//! 本地语音识别后端（sherpa-onnx 内嵌）。
//!
//! 支持两种引擎（`crate::model::EngineKind`）：SenseVoice（多语种自动检测）
//! 与 FireRedASR2 CTC（中英混说更强）。模型在管道启动时加载一次并常驻内存，
//! 之后每句话只做波形解码与推理。不走子进程方案：SenseVoice int8 模型
//! 约 230 MB、FireRedASR2 CTC 约 740 MB，每次冷载往往比转写本身还久。
//!
//! 推理是 CPU 密集的同步操作，通过 `tokio::task::spawn_blocking` 放进
//! blocking 线程池，避免阻塞异步 runtime。

use crate::error::TranscriberError;
use crate::model::EngineKind;
use crate::resource::effective_threads;
use crate::transcriber::{TranscribeResult, Transcriber};
use sherpa_onnx::{
    OfflineFireRedAsrCtcModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineSenseVoiceModelConfig,
};
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

/// 把配置语言归一化为 SenseVoice 可接受的值，空字符串按自动检测处理。
fn normalize_language(language: &str) -> &str {
    match language.trim() {
        "" => "auto",
        l => l,
    }
}

/// onnx 尾部元数据里 `model_type` 条目的字节模式。
///
/// 元数据是 protobuf 的 StringStringEntryProto，key 字段 tag 为 `0x0A`，
/// 长度 10（`model_type`），value 字段 tag 为 `0x12`，随后一字节长度再是值。
/// 实测 SenseVoice 与 FireRedASR2 CTC 两个模型的元数据都在文件末尾
/// 64 KB 内（分别距文件尾 343 与 3433 字节）。
const MODEL_TYPE_PATTERN: &[u8] = b"\x0a\x0amodel_type\x12";

/// 元数据扫描窗口：只读文件尾部这么多字节。
const METADATA_TAIL_BYTES: u64 = 64 * 1024;

/// 引擎展示名，用于错误文案。
fn engine_label(engine: EngineKind) -> &'static str {
    match engine {
        EngineKind::SenseVoice => "SenseVoice",
        EngineKind::FireRedAsrCtc => "FireRedASR2 CTC",
    }
}

/// 读取模型文件尾部元数据里的 `model_type`，判定它属于哪个引擎。
///
/// 返回 `None` 表示读不到或认不出：`[transcriber] model` 允许自定义路径，
/// 自制的 SenseVoice 模型可能没有该元数据，此时维持原有行为交由
/// sherpa-onnx 自行判断。
fn detect_engine_from_metadata(model_path: &std::path::Path) -> Option<EngineKind> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(model_path).ok()?;
    let size = file.metadata().ok()?.len();
    let window = size.min(METADATA_TAIL_BYTES) as usize;
    let mut buf = vec![0u8; window];
    file.seek(SeekFrom::Start(size - window as u64)).ok()?;
    file.read_exact(&mut buf).ok()?;

    let pattern_at = buf
        .windows(MODEL_TYPE_PATTERN.len())
        .position(|w| w == MODEL_TYPE_PATTERN)?;
    let len = *buf.get(pattern_at + MODEL_TYPE_PATTERN.len())? as usize;
    let value_at = pattern_at + MODEL_TYPE_PATTERN.len() + 1;
    let value = std::str::from_utf8(buf.get(value_at..value_at + len)?).ok()?;
    if value.starts_with("sense_voice") {
        Some(EngineKind::SenseVoice)
    } else if value.starts_with("fire-red-asr") {
        Some(EngineKind::FireRedAsrCtc)
    } else {
        None
    }
}

/// 校验模型文件与目标引擎一致。
///
/// 注册表名与自定义路径都可能指向与声明不符的模型文件，而 sherpa-onnx
/// 在配置与模型类型不匹配时直接退出进程（实测 `exit(-1)`），不是返回错误，
/// 构造识别器前必须先拦下来。
fn ensure_engine_matches(
    model_path: &std::path::Path,
    engine: EngineKind,
) -> Result<(), TranscriberError> {
    let Some(detected) = detect_engine_from_metadata(model_path) else {
        return Ok(());
    };
    if detected == engine {
        return Ok(());
    }
    Err(TranscriberError::ModelLoadFailed {
        reason: format!(
            "模型文件与识别引擎不匹配: {} 是 {} 模型，当前按 {} 加载。\
             请把 [transcriber] model 设为对应模型名，或在设置页重新选择模型",
            model_path.display(),
            engine_label(detected),
            engine_label(engine),
        ),
    })
}

impl SherpaTranscriber {
    /// 创建常驻识别器并立即加载模型。
    ///
    /// `model_dir` 应包含 `model.int8.onnx` 与 `tokens.txt`（见 `crate::model`
    /// 的下载逻辑），`engine` 决定用哪种识别器配置加载。
    /// `language` 仅 SenseVoice 使用：`"auto"` 自动检测（中/英/日/韩/粤），
    /// 或 `"zh"` / `"en"` / `"ja"` / `"ko"` / `"yue"` 指定，空字符串按 `"auto"`
    /// 处理；FireRedASR2 CTC 自带中英识别，该参数只透传到结果元数据。
    pub fn new(
        model_dir: PathBuf,
        language: String,
        threads: u32,
        engine: EngineKind,
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

        ensure_engine_matches(&model_path, engine)?;

        let mut config = OfflineRecognizerConfig::default();
        match engine {
            EngineKind::SenseVoice => {
                config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
                    model: Some(model_path.to_string_lossy().to_string()),
                    language: Some(normalize_language(&language).to_string()),
                    use_itn: true,
                };
            }
            EngineKind::FireRedAsrCtc => {
                config.model_config.fire_red_asr_ctc = OfflineFireRedAsrCtcModelConfig {
                    model: Some(model_path.to_string_lossy().to_string()),
                };
            }
        }
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
    fn transcribe_blocking(&self, audio_data: &[u8]) -> Result<TranscribeResult, TranscriberError> {
        if audio_data.is_empty() {
            return Err(TranscriberError::EmptyAudio);
        }

        let samples = crate::audio::decode_wav_to_f32(audio_data)
            .map_err(|e| TranscriberError::WavDecodeFailed(e.to_string()))?;

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
    fn backend(&self) -> &'static str {
        "local"
    }

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
        let err = SherpaTranscriber::new(
            PathBuf::from("/definitely/not/exists"),
            "zh".to_string(),
            0,
            EngineKind::SenseVoice,
        )
        .unwrap_err();
        assert!(matches!(err, TranscriberError::ModelLoadFailed { .. }));
        assert!(err.message().contains("模型文件不存在"));
    }

    #[test]
    fn test_new_missing_tokens_errors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.int8.onnx"), b"fake").unwrap();
        let err = SherpaTranscriber::new(
            dir.path().to_path_buf(),
            "zh".to_string(),
            0,
            EngineKind::SenseVoice,
        )
        .unwrap_err();
        assert!(matches!(err, TranscriberError::ModelLoadFailed { .. }));
        assert!(err.message().contains("词表不存在"));
    }

    /// 按真实 onnx 的字节布局拼一个只含 `model_type` 元数据的假模型文件。
    fn write_fake_model(dir: &std::path::Path, model_type: &str) {
        let mut bytes = MODEL_TYPE_PATTERN.to_vec();
        bytes.push(model_type.len() as u8);
        bytes.extend_from_slice(model_type.as_bytes());
        std::fs::write(dir.join("model.int8.onnx"), bytes).unwrap();
        std::fs::write(dir.join("tokens.txt"), b"<blk> 0\n").unwrap();
    }

    #[test]
    fn test_detect_engine_from_metadata_both_models() {
        let sv = tempfile::tempdir().unwrap();
        write_fake_model(sv.path(), "sense_voice_ctc");
        assert_eq!(
            detect_engine_from_metadata(&sv.path().join("model.int8.onnx")),
            Some(EngineKind::SenseVoice)
        );

        let fr = tempfile::tempdir().unwrap();
        write_fake_model(fr.path(), "fire-red-asr-2-ctc");
        assert_eq!(
            detect_engine_from_metadata(&fr.path().join("model.int8.onnx")),
            Some(EngineKind::FireRedAsrCtc)
        );
    }

    #[test]
    fn test_detect_engine_from_metadata_unknown_is_none() {
        let dir = tempfile::tempdir().unwrap();
        write_fake_model(dir.path(), "some-other-model");
        assert_eq!(
            detect_engine_from_metadata(&dir.path().join("model.int8.onnx")),
            None
        );

        let empty = tempfile::tempdir().unwrap();
        std::fs::write(empty.path().join("model.int8.onnx"), b"\x00\x01\x02").unwrap();
        assert_eq!(
            detect_engine_from_metadata(&empty.path().join("model.int8.onnx")),
            None
        );
    }

    #[test]
    fn test_new_rejects_engine_model_mismatch_before_loading() {
        // FireRedASR2 模型文件按 SenseVoice 引擎加载：历史上会让 sherpa-onnx
        // 直接 exit(-1) 终止进程，必须在构造识别器前返回错误。
        let dir = tempfile::tempdir().unwrap();
        write_fake_model(dir.path(), "fire-red-asr-2-ctc");
        let err = SherpaTranscriber::new(
            dir.path().to_path_buf(),
            "zh".to_string(),
            0,
            EngineKind::SenseVoice,
        )
        .unwrap_err();
        assert!(matches!(err, TranscriberError::ModelLoadFailed { .. }));
        assert!(err.message().contains("不匹配"));

        // 反方向：SenseVoice 模型文件按 FireRedASR2 引擎加载。
        let dir2 = tempfile::tempdir().unwrap();
        write_fake_model(dir2.path(), "sense_voice_ctc");
        let err = SherpaTranscriber::new(
            dir2.path().to_path_buf(),
            "zh".to_string(),
            0,
            EngineKind::FireRedAsrCtc,
        )
        .unwrap_err();
        assert!(err.message().contains("不匹配"));
    }
}
