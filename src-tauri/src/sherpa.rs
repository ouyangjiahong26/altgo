//! SenseVoice 本地语音识别后端（sherpa-onnx 内嵌）。
//!
//! sherpa-onnx 编译进主程序：模型在管道启动时加载一次并常驻内存，
//! 之后每句话只做波形解码与推理。不走子进程方案——SenseVoice int8 模型
//! 约 230MB，每次冷载往往比转写本身还久。
//!
//! 推理是 CPU 密集的同步操作，通过 `tokio::task::spawn_blocking` 放进
//! blocking 线程池，避免阻塞异步 runtime。

use crate::error::TranscriberError;
use crate::resource::effective_threads;
use crate::transcriber::{TranscribeResult, Transcriber};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

/// SenseVoice 离线识别器。
///
/// `Clone` 极廉价（`Arc<OfflineRecognizer>` 共享），识别器本身线程安全
/// （sherpa-onnx 的 `unsafe impl Send + Sync`），可并发转写多个流。
#[derive(Clone)]
pub struct SherpaTranscriber {
    recognizer: Arc<OfflineRecognizer>,
    language: String,
}

impl std::fmt::Debug for SherpaTranscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SherpaTranscriber")
            .field("language", &self.language)
            .finish()
    }
}

/// 把配置语言归一化为 SenseVoice 可接受的值；空字符串按自动检测处理。
fn normalize_language(language: &str) -> &str {
    match language.trim() {
        "" => "auto",
        l => l,
    }
}

impl SherpaTranscriber {
    /// 创建常驻识别器并立即加载模型。
    ///
    /// `model_dir` 应包含 SenseVoice 模型的 `model.int8.onnx` 与 `tokens.txt`
    /// （见 `crate::model` 的下载逻辑）。
    /// `language`：`"auto"` 自动检测（中/英/日/韩/粤），或 `"zh"` / `"en"` /
    /// `"ja"` / `"ko"` / `"yue"` 指定；空字符串按 `"auto"` 处理。
    pub fn new(
        model_dir: PathBuf,
        language: String,
        threads: u32,
    ) -> Result<Self, TranscriberError> {
        let model_path = model_dir.join("model.int8.onnx");
        let tokens_path = model_dir.join("tokens.txt");
        if !model_path.exists() {
            return Err(TranscriberError::ModelLoadFailed {
                reason: format!("模型文件不存在: {}", model_path.display()),
            });
        }
        if !tokens_path.exists() {
            return Err(TranscriberError::ModelLoadFailed {
                reason: format!("模型词表不存在: {}", tokens_path.display()),
            });
        }

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(model_path.to_string_lossy().to_string()),
            language: Some(normalize_language(&language).to_string()),
            use_itn: true,
        };
        config.model_config.tokens = Some(tokens_path.to_string_lossy().to_string());
        config.model_config.num_threads = effective_threads(threads) as i32;

        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            TranscriberError::ModelLoadFailed {
                reason: format!("sherpa-onnx 加载模型失败: {}", model_dir.display()),
            }
        })?;

        Ok(Self {
            recognizer: Arc::new(recognizer),
            language,
        })
    }

    /// 同步转写：WAV 解码 → 推理 → 返回文本。调用方应放入 blocking 线程池。
    pub fn transcribe_blocking(
        &self,
        audio_data: &[u8],
    ) -> Result<TranscribeResult, TranscriberError> {
        if audio_data.is_empty() {
            return Err(TranscriberError::EmptyAudio);
        }

        let samples = crate::audio::decode_wav_to_f32(audio_data)
            .map_err(TranscriberError::WavDecodeFailed)?;

        let stream = self.recognizer.create_stream();
        stream.accept_waveform(crate::recorder::SAMPLE_RATE as i32, &samples);
        self.recognizer.decode(&stream);

        let text = stream
            .get_result()
            .map(|r| r.text)
            .unwrap_or_default()
            .trim()
            .to_string();

        Ok(TranscribeResult {
            text,
            language: self.language.clone(),
        })
    }
}

impl Transcriber for SherpaTranscriber {
    fn transcribe<'life0, 'life1>(
        &'life0 self,
        audio: &'life1 [u8],
        on_progress: Arc<dyn Fn(f32) + Send + Sync>,
    ) -> Pin<Box<dyn Future<Output = Result<TranscribeResult, TranscriberError>> + Send + 'life0>>
    where
        'life1: 'life0,
    {
        let this = self.clone();
        let audio = audio.to_vec();
        Box::pin(async move {
            if audio.is_empty() {
                return Err(TranscriberError::EmptyAudio);
            }
            let result = tokio::task::spawn_blocking(move || this.transcribe_blocking(&audio))
                .await
                .map_err(|e| TranscriberError::ModelLoadFailed {
                    reason: format!("推理任务执行失败: {}", e),
                })??;
            (on_progress)(1.0);
            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_language() {
        assert_eq!(normalize_language(""), "auto");
        assert_eq!(normalize_language("  "), "auto");
        assert_eq!(normalize_language("zh"), "zh");
        assert_eq!(normalize_language("en"), "en");
        assert_eq!(normalize_language("auto"), "auto");
    }

    #[test]
    fn test_new_missing_model_errors() {
        let err =
            SherpaTranscriber::new(PathBuf::from("/definitely/not/exists"), "zh".to_string(), 0)
                .unwrap_err();
        assert!(matches!(err, TranscriberError::ModelLoadFailed { .. }));
        assert!(err.message().contains("模型文件不存在"));
    }

    #[test]
    fn test_new_missing_tokens_errors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.int8.onnx"), b"fake").unwrap();
        let err =
            SherpaTranscriber::new(dir.path().to_path_buf(), "zh".to_string(), 0).unwrap_err();
        assert!(matches!(err, TranscriberError::ModelLoadFailed { .. }));
        assert!(err.message().contains("词表不存在"));
    }
}
