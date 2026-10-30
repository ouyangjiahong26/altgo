//! 本地语音识别后端（sherpa-onnx 内嵌）。
//!
//! 支持两种引擎（`crate::model::EngineKind`）：SenseVoice（多语种自动检测）
//! 与 FireRedASR2 CTC（中英混说更强）。模型在管道启动时于后台加载一次并常驻内存，
//! 加载期间监听与录音已经可用；之后每句话只做波形解码与推理。不走子进程方案：
//! SenseVoice int8 模型约 230 MB、FireRedASR2 CTC 约 740 MB，每次冷载往往比转写本身还久。
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
use std::sync::{Arc, OnceLock};

mod segmentation;

use segmentation::{
    collect_vad_segments, create_detector, needs_segmentation, split_on_silence,
    split_on_speech_ranges, vad_model_ready, SegmentRange, SpeechRange, SEGMENT_LIMIT_MS,
    VAD_CHUNK_SAMPLES,
};

/// SenseVoice 离线识别器。
///
/// `Clone` 极廉价：识别器按需加载并共享，识别器本身线程安全
/// （sherpa-onnx 的 `unsafe impl Send + Sync`），可并发转写多个流。
#[derive(Clone)]
pub struct SherpaTranscriber {
    /// 按需加载的识别器槽位。失败原因记在槽里，后续调用返回同一个错误，不重试。
    ///
    /// 加载要数秒（SenseVoice int8 约 230 MB），放在构造期会把监听与录音一起挡在
    /// 后面：用户刚打开软件就按触发键，那几秒里没有任何接收方。
    recognizer: Arc<OnceLock<Result<Arc<OfflineRecognizer>, String>>>,
    /// 加载识别器所需的配置，加载成功后不再使用。
    config: Arc<OfflineRecognizerConfig>,
    /// 只用于加载失败的提示文案。
    model_dir: PathBuf,
    language: String,
    /// 识别引擎：决定输入时长上限。
    engine: EngineKind,
    /// 随安装包分发的 silero-vad 模型路径。缺失时退回 RMS 静音切分。
    vad_model: Option<PathBuf>,
    /// VAD 不可用只记一次日志，避免每次长录音都刷屏。
    vad_warned: Arc<OnceLock<()>>,
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

/// FireRedASR2 CTC 单段最长时长（毫秒）。
///
/// 该模型的 encoder 按 5000 帧导出相对位置编码，特征按 25 帧/秒产生，单段
/// 超过 200 秒后 mhsa 的相加节点广播失败。ONNX Runtime 抛出的 C++ 异常穿过
/// FFI 边界时 Rust 无法捕获，运行时直接终止进程。分段把每段限制在 200 秒
/// 以内，绕过这个硬上限。
const FIRE_RED_MAX_AUDIO_MS: u64 = 200_000;

/// 引擎能安全接受的最长单段音频（毫秒）。`None` 表示没有已知的硬上限。
fn max_audio_ms(engine: EngineKind) -> Option<u64> {
    match engine {
        EngineKind::FireRedAsrCtc => Some(FIRE_RED_MAX_AUDIO_MS),
        EngineKind::SenseVoice => None,
    }
}

/// 会话配置文件里关闭 CPU 内存 arena 的开关。
///
/// ONNX Runtime 的 CPU arena 会把每次推理的工作集留在进程里，不归还系统。实测
/// 741 MB 的 FireRedASR2 CTC 在 180 秒段上，默认配置驻留内存从 8.0 GB 涨到
/// 16.2 GB 后一直保持，长录音分段转写会在闲置时继续占着十几 GB。关掉 arena 后
/// 每次推理结束回落到模型基线（约 1.1 GB），代价是长音频每段慢约 23%，十秒级
/// 短语音实测无差别。见 k2-fsa/sherpa-onnx#4138。
const SESSION_CONFIG: &str = "EnableCpuMemArena=0\n";

/// 把会话配置文件写进模型目录，返回带该文件的 provider 字符串。
///
/// sherpa-onnx 只在 `provider` 字符串的 `provider_name:config_path` 形式里读取
/// 这些会话选项，C API 没有对应字段。写入失败（自定义模型目录只读等）时返回
/// `None`，退回默认 provider，内存行为与升级前一致，不影响推理。
fn session_provider(model_dir: &std::path::Path) -> Option<String> {
    let dir = std::fs::canonicalize(model_dir).ok()?;
    let path = dir.join("session.conf");
    if let Err(e) = std::fs::write(&path, SESSION_CONFIG) {
        tracing::warn!(
            error = %e,
            path = %path.display(),
            "failed to write sherpa-onnx session config, using defaults"
        );
        return None;
    }
    Some(format!("cpu:{}", path.display()))
}

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
    /// 创建识别器：只校验模型文件与引擎是否匹配，真正的加载留给
    /// [`Self::warm_up`] 或首次转写。
    ///
    /// `model_dir` 应包含 `model.int8.onnx` 与 `tokens.txt`（见 `crate::model`
    /// 的下载逻辑），`engine` 决定用哪种识别器配置加载。
    /// `language` 仅 SenseVoice 使用：`"auto"` 自动检测（中/英/日/韩/粤），
    /// 或 `"zh"` / `"en"` / `"ja"` / `"ko"` / `"yue"` 指定，空字符串按 `"auto"`
    /// 处理；FireRedASR2 CTC 自带中英识别，该参数只透传到结果元数据。
    ///
    /// `vad_model` 是随安装包分发的 silero-vad 模型路径，用于长录音切分。
    /// `None` 时退回 RMS 静音切分，转写行为不受影响。
    pub fn new(
        model_dir: PathBuf,
        language: String,
        threads: u32,
        engine: EngineKind,
        vad_model: Option<PathBuf>,
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
        config.model_config.provider = session_provider(&model_dir);

        Ok(Self {
            recognizer: Arc::new(OnceLock::new()),
            config: Arc::new(config),
            model_dir,
            language,
            engine,
            vad_model: vad_model.filter(|path| vad_model_ready(path)),
            vad_warned: Arc::new(OnceLock::new()),
        })
    }

    /// 取识别器。首次调用真正读模型，之后直接返回同一个实例或同一个失败原因。
    fn recognizer(&self) -> Result<&Arc<OfflineRecognizer>, TranscriberError> {
        let loaded = self.recognizer.get_or_init(|| {
            let started = std::time::Instant::now();
            let created = OfflineRecognizer::create(&self.config)
                .map(Arc::new)
                .ok_or_else(|| format!("sherpa-onnx 加载模型失败: {}", self.model_dir.display()));
            match &created {
                Ok(_) => tracing::info!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "local model loaded"
                ),
                Err(reason) => tracing::warn!(reason, "local model load failed"),
            }
            created
        });
        match loaded {
            Ok(recognizer) => Ok(recognizer),
            Err(reason) => Err(TranscriberError::ModelLoadFailed {
                reason: reason.clone(),
            }),
        }
    }

    /// 预热：把模型加载挪到后台线程，监听与录音不必等它。
    ///
    /// 失败原因留在槽里，首次转写会拿到同一个错误，不必在这里处理。
    pub fn warm_up(&self) {
        let _ = self.recognizer();
    }

    /// 同步转写：WAV 解码 → 推理 → 返回文本。调用方应放入 blocking 线程池。
    ///
    /// 短音频直接单次推理；超过触发阈值的长录音先切分，逐段识别后拼接。
    fn transcribe_blocking(
        &self,
        audio_data: &[u8],
        on_progress: &(dyn Fn(f32) + Send + Sync),
    ) -> Result<TranscribeResult, TranscriberError> {
        if audio_data.is_empty() {
            return Err(TranscriberError::EmptyAudio);
        }

        let samples = crate::audio::decode_wav_to_f32(audio_data)
            .map_err(|e| TranscriberError::WavDecodeFailed(e.to_string()))?;

        let duration_ms = samples.len() as u64 * 1000 / crate::recorder::SAMPLE_RATE as u64;

        if needs_segmentation(duration_ms) {
            self.transcribe_segmented(&samples, on_progress)
        } else {
            let text = self.transcribe_single(&samples)?;
            (on_progress)(1.0);
            Ok(TranscribeResult {
                text,
                language: self.language.clone(),
            })
        }
    }

    /// 单段推理：把采样交给识别器，返回识别文本。
    fn transcribe_single(&self, samples: &[f32]) -> Result<String, TranscriberError> {
        let recognizer = self.recognizer()?;
        let stream = recognizer.create_stream();
        stream.accept_waveform(crate::recorder::SAMPLE_RATE as i32, samples);
        recognizer.decode(&stream);

        let text = stream
            .get_result()
            .map(|r| r.text)
            .unwrap_or_default()
            .trim()
            .to_string();
        Ok(text)
    }

    /// VAD 不可用时只记一次日志，避免每次长录音都刷屏。
    fn warn_vad_unavailable(&self, reason: &str) {
        if self.vad_warned.set(()).is_ok() {
            tracing::warn!(
                reason,
                "silero-vad unavailable, falling back to RMS segmentation"
            );
        }
    }

    /// 用 silero-vad 找出音频里的语音区间。
    ///
    /// 返回 `None` 表示 VAD 不可用或没有检出语音，调用方退回 RMS 切分。
    /// 分块送入并及时取走语音段，长录音的内存占用不随输入长度增长。
    fn vad_speech_ranges(&self, samples: &[f32]) -> Option<Vec<SpeechRange>> {
        let Some(model) = self.vad_model.as_deref() else {
            self.warn_vad_unavailable("未找到可用的 silero-vad 模型（缺失或校验失败）");
            return None;
        };
        let Some(detector) = create_detector(model) else {
            self.warn_vad_unavailable(&format!("加载 silero-vad 失败: {}", model.display()));
            return None;
        };

        let mut ranges = Vec::new();
        for chunk in samples.chunks(VAD_CHUNK_SAMPLES) {
            detector.accept_waveform(chunk);
            collect_vad_segments(&detector, samples.len(), &mut ranges);
        }
        detector.flush();
        collect_vad_segments(&detector, samples.len(), &mut ranges);

        if ranges.is_empty() {
            tracing::debug!("silero-vad detected no speech, falling back to RMS segmentation");
            return None;
        }
        Some(ranges)
    }

    /// 切分长录音：优先用 VAD 语音边界，VAD 不可用或没有检出语音时退回 RMS。
    fn split_audio(&self, samples: &[f32]) -> (Vec<SegmentRange>, bool) {
        let sample_rate = crate::recorder::SAMPLE_RATE;
        if let Some(speech) = self.vad_speech_ranges(samples) {
            let ranges = split_on_speech_ranges(samples.len(), sample_rate, &speech);
            if !ranges.is_empty() {
                return (ranges, true);
            }
        }
        // 引擎自身的单段上限留作兜底：VAD 误判时也不会把超长音频送进推理。
        let limit_ms = SEGMENT_LIMIT_MS.min(max_audio_ms(self.engine).unwrap_or(u64::MAX));
        (split_on_silence(samples, sample_rate, limit_ms), false)
    }

    /// 分段转写：按语音边界切分，逐段识别，按顺序拼接文本。
    fn transcribe_segmented(
        &self,
        samples: &[f32],
        on_progress: &(dyn Fn(f32) + Send + Sync),
    ) -> Result<TranscribeResult, TranscriberError> {
        let (segments, used_vad) = self.split_audio(samples);

        tracing::info!(
            segments = segments.len(),
            used_vad,
            "long audio split into segments"
        );

        let mut texts = Vec::with_capacity(segments.len());
        for (i, seg) in segments.iter().enumerate() {
            let chunk = &samples[seg.start..seg.end];
            let text = self.transcribe_single(chunk)?;
            if !text.is_empty() {
                texts.push(text);
            }
            (on_progress)((i + 1) as f32 / segments.len() as f32);
        }

        // 中文各段直接拼接（中文无空格分词），英文段间加空格。
        // 简单策略：如果段尾或段首是 ASCII 字母/数字，则插入空格。
        let mut result = String::new();
        for (i, text) in texts.iter().enumerate() {
            if i > 0 && needs_space_between(result.chars().last(), text.chars().next()) {
                result.push(' ');
            }
            result.push_str(text);
        }

        Ok(TranscribeResult {
            text: result.trim().to_string(),
            language: self.language.clone(),
        })
    }
}

/// 拼接两段文本时是否需要空格：前段结尾和后段开头都是 ASCII 可打印字符
/// （字母、数字、标点）时加空格，中文之间不加。
fn needs_space_between(prev: Option<char>, next: Option<char>) -> bool {
    match (prev, next) {
        (Some(p), Some(n)) => p.is_ascii() && n.is_ascii(),
        _ => false,
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
        let progress = Arc::clone(&on_progress);
        Box::pin(async move {
            if audio.is_empty() {
                return Err(TranscriberError::EmptyAudio);
            }
            let result =
                tokio::task::spawn_blocking(move || this.transcribe_blocking(&audio, &*progress))
                    .await
                    .map_err(|e| TranscriberError::ModelLoadFailed {
                        reason: format!("推理任务执行失败: {}", e),
                    })??;
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
            None,
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
            None,
        )
        .unwrap_err();
        assert!(matches!(err, TranscriberError::ModelLoadFailed { .. }));
        assert!(err.message().contains("词表不存在"));
    }

    #[test]
    fn test_new_writes_session_config_disabling_cpu_mem_arena() {
        // CPU arena 不归还工作集，长录音分段转写会长期占着十几 GB。
        // 会话选项只能经 provider 字符串里的配置文件传入。
        let dir = tempfile::tempdir().unwrap();
        write_fake_model(dir.path(), "sense_voice_ctc");
        let transcriber = SherpaTranscriber::new(
            dir.path().to_path_buf(),
            String::new(),
            0,
            EngineKind::SenseVoice,
            None,
        )
        .unwrap();

        let content = std::fs::read_to_string(dir.path().join("session.conf")).unwrap();
        assert!(content.contains("EnableCpuMemArena=0"));
        assert!(
            transcriber
                .config
                .model_config
                .provider
                .as_deref()
                .is_some_and(|p| p.starts_with("cpu:")),
            "provider 应指向会话配置文件，实际为 {:?}",
            transcriber.config.model_config.provider
        );
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
            None,
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
            None,
        )
        .unwrap_err();
        assert!(err.message().contains("不匹配"));
    }

    #[test]
    fn test_only_fire_red_has_length_guard() {
        assert_eq!(max_audio_ms(EngineKind::SenseVoice), None);
        assert_eq!(
            max_audio_ms(EngineKind::FireRedAsrCtc),
            Some(FIRE_RED_MAX_AUDIO_MS)
        );
    }

    #[test]
    fn test_vad_speech_ranges_without_model_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        write_fake_model(dir.path(), "sense_voice_ctc");
        let transcriber = SherpaTranscriber::new(
            dir.path().to_path_buf(),
            String::new(),
            0,
            EngineKind::SenseVoice,
            None,
        )
        .unwrap();
        let samples = vec![0.1f32; 16000];
        assert!(transcriber.vad_speech_ranges(&samples).is_none());
    }

    #[test]
    fn test_split_audio_falls_back_to_rms_without_vad() {
        let dir = tempfile::tempdir().unwrap();
        write_fake_model(dir.path(), "sense_voice_ctc");
        let transcriber = SherpaTranscriber::new(
            dir.path().to_path_buf(),
            String::new(),
            0,
            EngineKind::SenseVoice,
            None,
        )
        .unwrap();

        // 72 秒：5 秒语音与 1 秒静音交替。
        let sr = 16000usize;
        let speech = vec![0.5f32; sr * 5];
        let silence = vec![0.0001f32; sr];
        let mut samples = Vec::new();
        for _ in 0..12 {
            samples.extend_from_slice(&speech);
            samples.extend_from_slice(&silence);
        }

        let (ranges, used_vad) = transcriber.split_audio(&samples);
        assert!(!used_vad);
        assert!(ranges.len() >= 2);
        let limit = sr * SEGMENT_LIMIT_MS as usize / 1000;
        for range in &ranges {
            assert!(range.end - range.start <= limit);
        }
    }

    #[test]
    fn test_vad_speech_ranges_loads_bundled_model() {
        // 真正加载随仓库分发的 silero-vad，静音输入不应产生语音区间。
        let model =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../resources/vad/silero_vad.onnx");
        assert!(model.is_file(), "仓库应包含 {}", model.display());
        let dir = tempfile::tempdir().unwrap();
        write_fake_model(dir.path(), "sense_voice_ctc");
        let transcriber = SherpaTranscriber::new(
            dir.path().to_path_buf(),
            String::new(),
            0,
            EngineKind::SenseVoice,
            Some(model),
        )
        .unwrap();

        let silence = vec![0.0f32; 16000 * 2];
        assert!(transcriber.vad_speech_ranges(&silence).is_none());
        assert!(
            transcriber.vad_warned.get().is_none(),
            "随安装包分发的 silero-vad 应能加载"
        );
    }

    #[test]
    fn test_needs_space_between() {
        assert!(needs_space_between(Some('a'), Some('b')));
        assert!(needs_space_between(Some('1'), Some('2')));
        assert!(!needs_space_between(Some('中'), Some('文')));
        assert!(!needs_space_between(Some('a'), Some('中')));
        assert!(!needs_space_between(Some('中'), Some('a')));
        assert!(!needs_space_between(None, Some('a')));
        assert!(!needs_space_between(Some('a'), None));
    }
}
