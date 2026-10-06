//! altgo 流水线的结构化错误类型。
//!
//! 区分致命错误（停管道）与可恢复错误（降级继续）。`user_error()` 把错误映射为
//! 稳定错误码（`UserFacingError`），经事件通道交给前端字典翻译，Rust 不再向事件
//! 通道发中文文案。`message()` 的中文文案仅剩润色连接测试（`describe_test_error`）
//! 与测试消费。

use std::collections::BTreeMap;
use std::path::PathBuf;

/// 经事件通道传给前端的用户可见错误：`code` 是稳定错误码，前端字典按语言翻译。
/// `params` 供模板占位符（如 `{reason}`）插值。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserFacingError {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<String, String>>,
}

impl UserFacingError {
    /// 无参数错误码的便捷构造。
    fn bare(code: &str) -> Self {
        Self {
            code: code.to_string(),
            params: None,
        }
    }

    /// 带单参数错误码的便捷构造。
    fn with(code: &str, key: &str, value: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            params: Some(BTreeMap::from([(key.to_string(), value.into())])),
        }
    }
}

/// 顶层流水线错误，区分致命 / 可恢复两类。
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("{0}")]
    Fatal(FatalError),

    #[error("{0}")]
    Recoverable(RecoverableError),
}

impl PipelineError {
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }

    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::Recoverable(_))
    }

    /// 把 `TranscriberError` 包装为致命的 `TranscriberInitFailed`（构造期失败）
    /// 或可恢复的 `TranscriptionFailed`（运行期失败）。调用方经由该方法选择。
    pub fn fatal_transcriber(e: TranscriberError) -> Self {
        Self::Fatal(FatalError::TranscriberInitFailed(e))
    }

    /// 把 `PolisherError` 包装为致命的 `PolisherInitFailed`（构造期）
    /// 或可恢复的 `PolishingFailed`（运行期）。
    pub fn fatal_polisher(e: PolisherError) -> Self {
        Self::Fatal(FatalError::PolisherInitFailed(e))
    }

    /// 返回面向用户的中文错误消息。
    pub fn message(&self) -> String {
        match self {
            Self::Fatal(e) => e.message(),
            Self::Recoverable(e) => e.message(),
        }
    }

    /// 映射为前端可翻译的错误码，分发到子枚举。
    pub fn user_error(&self) -> UserFacingError {
        match self {
            Self::Fatal(e) => e.user_error(),
            Self::Recoverable(e) => e.user_error(),
        }
    }
}

/// 应终止流水线的致命错误。
#[derive(Debug, thiserror::Error)]
pub enum FatalError {
    #[error("Model not found: {model}")]
    ModelNotFound {
        model: String,
        searched: Vec<PathBuf>,
    },

    #[error("API authentication failed: {service} returned {status}")]
    ApiAuthFailed { service: &'static str, status: u16 },

    #[error("Key listener failed to start: {backend} - {reason}")]
    KeyListenerFailed { backend: String, reason: String },

    #[error("Transcriber initialization failed: {0}")]
    TranscriberInitFailed(#[from] TranscriberError),

    #[error("Polisher initialization failed: {0}")]
    PolisherInitFailed(#[from] PolisherError),

    #[error("Recorder initialization failed: {0}")]
    RecorderInitFailed(#[from] RecorderError),
}

impl FatalError {
    pub fn message(&self) -> String {
        match self {
            Self::ModelNotFound { model, searched } => {
                let paths: Vec<String> = searched.iter().map(|p| p.display().to_string()).collect();
                format!(
                    "本地模型未找到（配置值：{model}）。\n搜索路径：{}\n请在 GUI 设置中下载模型，或将 [transcriber] model 设为已下载模型的名称（如 \"sense-voice\"）、包含 model.int8.onnx 与 tokens.txt 的目录，或 model.int8.onnx 文件路径。",
                    paths.join("\n  ")
                )
            }
            Self::ApiAuthFailed { service, status } => {
                format!(
                    "{} API 认证失败（HTTP {}）。请检查 API 密钥配置。",
                    service, status
                )
            }
            Self::KeyListenerFailed { backend, reason } => {
                format!("按键监听器启动失败（{}）：{}", backend, reason)
            }
            Self::TranscriberInitFailed(e) => e.message(),
            Self::PolisherInitFailed(e) => e.message(),
            Self::RecorderInitFailed(e) => e.message(),
        }
    }

    /// 映射为前端可翻译的错误码，包装变体透传内层错误的码。
    pub fn user_error(&self) -> UserFacingError {
        match self {
            Self::ModelNotFound { model, searched } => UserFacingError {
                code: "fatal.model_not_found".into(),
                params: Some(BTreeMap::from([
                    ("model".to_string(), model.clone()),
                    (
                        "searched".to_string(),
                        searched
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                ])),
            },
            Self::ApiAuthFailed { service, status } => UserFacingError {
                code: "fatal.api_auth_failed".into(),
                params: Some(BTreeMap::from([
                    ("service".to_string(), service.to_string()),
                    ("status".to_string(), status.to_string()),
                ])),
            },
            Self::KeyListenerFailed { backend, reason } => UserFacingError {
                code: "fatal.key_listener_failed".into(),
                params: Some(BTreeMap::from([
                    ("backend".to_string(), backend.clone()),
                    ("reason".to_string(), reason.clone()),
                ])),
            },
            Self::TranscriberInitFailed(e) => e.user_error(),
            Self::PolisherInitFailed(e) => e.user_error(),
            Self::RecorderInitFailed(e) => e.user_error(),
        }
    }
}

/// 允许优雅降级的可恢复错误。
#[derive(Debug, thiserror::Error)]
pub enum RecoverableError {
    #[error("Transcription failed: {0}")]
    TranscriptionFailed(#[from] TranscriberError),

    #[error("Polishing failed: {0}")]
    PolishingFailed(#[from] PolisherError),

    #[error("Recording failed: {0}")]
    RecordingFailed(#[from] RecorderError),

    #[error("Empty transcription result")]
    EmptyTranscription,
}

impl RecoverableError {
    pub fn message(&self) -> String {
        match self {
            Self::TranscriptionFailed(e) => e.message(),
            Self::PolishingFailed(e) => e.message(),
            Self::RecordingFailed(e) => e.message(),
            Self::EmptyTranscription => "转写结果为空，请重试。".to_string(),
        }
    }

    /// 映射为前端可翻译的错误码，包装变体透传内层错误的码。
    pub fn user_error(&self) -> UserFacingError {
        match self {
            Self::TranscriptionFailed(e) => e.user_error(),
            Self::PolishingFailed(e) => e.user_error(),
            Self::RecordingFailed(e) => e.user_error(),
            Self::EmptyTranscription => UserFacingError::bare("transcription.empty"),
        }
    }
}

/// 转写器专属错误。
#[derive(Debug, thiserror::Error)]
pub enum TranscriberError {
    #[error("Empty audio data")]
    EmptyAudio,

    #[error("transcription returned empty text for non-trivial audio")]
    EmptyResult,

    #[error("ASR API returned no usable content")]
    EmptyResponse,

    #[error("failed to load local model: {reason}")]
    ModelLoadFailed { reason: String },

    #[error("audio decode error: {0}")]
    WavDecodeFailed(String),

    #[error("invalid ASR API base URL: {0}")]
    InvalidBaseUrl(String),

    #[error("ASR API returned {status}: {body}")]
    ApiError { status: u16, body: String },

    #[error("ASR HTTP client error: {0}")]
    HttpError(String),

    #[error("ASR JSON parse error: {0}")]
    JsonError(String),
}

impl TranscriberError {
    pub fn message(&self) -> String {
        match self {
            Self::EmptyAudio | Self::EmptyResult => "音频数据为空，请重新录音。".to_string(),
            Self::EmptyResponse => "在线识别返回了空内容，请重试。".to_string(),
            Self::ModelLoadFailed { reason } => format!("本地模型加载失败：{}", reason),
            Self::WavDecodeFailed(msg) => format!("音频解码失败：{}", msg),
            Self::InvalidBaseUrl(url) => {
                format!(
                    "在线识别 API 地址无效：'{}'。请填写完整 URL（如 https://token-plan-cn.xiaomimimo.com/v1）。",
                    url
                )
            }
            Self::ApiError { status, body } => {
                format!("在线识别 API 错误（HTTP {}）：{}", status, body)
            }
            Self::HttpError(msg) => format!("在线识别请求失败：{}", msg),
            Self::JsonError(msg) => format!("在线识别响应解析失败：{}", msg),
        }
    }

    /// 映射为前端可翻译的错误码。
    pub fn user_error(&self) -> UserFacingError {
        match self {
            Self::EmptyAudio => UserFacingError::bare("transcriber.empty_audio"),
            // 复用既有前端词条 error.transcription.empty：录音有效但识别结果为空。
            Self::EmptyResult => UserFacingError::bare("transcription.empty"),
            Self::EmptyResponse => UserFacingError::bare("transcriber.empty_response"),
            Self::ModelLoadFailed { reason } => {
                UserFacingError::with("transcriber.model_load_failed", "reason", reason.clone())
            }
            Self::WavDecodeFailed(msg) => {
                UserFacingError::with("transcriber.wav_decode_failed", "reason", msg.clone())
            }
            Self::InvalidBaseUrl(url) => {
                UserFacingError::with("transcriber.invalid_base_url", "url", url.clone())
            }
            Self::ApiError { status, body } => UserFacingError {
                code: "transcriber.api_error".into(),
                params: Some(BTreeMap::from([
                    ("status".to_string(), status.to_string()),
                    ("body".to_string(), body.clone()),
                ])),
            },
            Self::HttpError(msg) => {
                UserFacingError::with("transcriber.http_error", "detail", msg.clone())
            }
            Self::JsonError(msg) => {
                UserFacingError::with("transcriber.json_error", "detail", msg.clone())
            }
        }
    }
}

/// 润色器专属错误。
#[derive(Debug, thiserror::Error)]
pub enum PolisherError {
    #[error("Unknown protocol: {protocol}")]
    UnknownProtocol { protocol: String },

    #[error("invalid API base URL: {0}")]
    InvalidBaseUrl(String),

    #[error("API key not configured")]
    MissingApiKey,

    #[error("Rate limited")]
    RateLimited,

    #[error("LLM API returned {status}: {body}")]
    ApiError { status: u16, body: String },

    #[error("LLM returned empty response")]
    EmptyResponse,

    #[error("HTTP client error: {0}")]
    HttpError(String),

    #[error("JSON parse error: {0}")]
    JsonError(String),

    #[error("All retry attempts exhausted")]
    RetriesExhausted,
}

impl PolisherError {
    pub fn message(&self) -> String {
        match self {
            Self::UnknownProtocol { protocol } => {
                format!(
                    "未知的润色协议: '{}'。请使用 'openai' 或 'anthropic'。",
                    protocol
                )
            }
            Self::InvalidBaseUrl(url) => {
                format!(
                    "润色 API 地址无效：'{url}'。请填写完整的 URL（如 https://api.deepseek.com）。"
                )
            }
            Self::MissingApiKey => "润色 API 密钥未配置。请在设置中添加 API 密钥。".to_string(),
            Self::RateLimited => "API 请求频率受限，请稍后重试。".to_string(),
            Self::ApiError { status, body } => {
                format!("LLM API 错误（HTTP {}）：{}", status, body)
            }
            Self::EmptyResponse => "LLM 返回空响应。".to_string(),
            Self::HttpError(msg) => format!("HTTP 请求失败：{}", msg),
            Self::JsonError(msg) => format!("JSON 解析失败：{}", msg),
            Self::RetriesExhausted => "所有重试尝试均失败。".to_string(),
        }
    }

    /// 映射为前端可翻译的错误码。
    pub fn user_error(&self) -> UserFacingError {
        match self {
            Self::UnknownProtocol { protocol } => {
                UserFacingError::with("polisher.unknown_protocol", "protocol", protocol.clone())
            }
            Self::InvalidBaseUrl(url) => {
                UserFacingError::with("polisher.invalid_base_url", "url", url.clone())
            }
            Self::MissingApiKey => UserFacingError::bare("polisher.missing_api_key"),
            Self::RateLimited => UserFacingError::bare("polisher.rate_limited"),
            Self::ApiError { status, body } => UserFacingError {
                code: "polisher.api_error".into(),
                params: Some(BTreeMap::from([
                    ("status".to_string(), status.to_string()),
                    ("body".to_string(), body.clone()),
                ])),
            },
            Self::EmptyResponse => UserFacingError::bare("polisher.empty_response"),
            Self::HttpError(msg) => {
                UserFacingError::with("polisher.http_error", "detail", msg.clone())
            }
            Self::JsonError(msg) => {
                UserFacingError::with("polisher.json_error", "detail", msg.clone())
            }
            Self::RetriesExhausted => UserFacingError::bare("polisher.retries_exhausted"),
        }
    }
}

/// 录音器专属错误。
#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error("Failed to start recording: {0}")]
    StartFailed(String),

    #[error("Failed to stop recording: {0}")]
    StopFailed(String),

    #[error("Audio capture error: {0}")]
    CaptureFailed(String),

    #[error("Empty recording")]
    EmptyRecording,
}

impl RecorderError {
    pub fn message(&self) -> String {
        match self {
            Self::StartFailed(msg) => format!("启动录音失败：{}", msg),
            Self::StopFailed(msg) => format!("停止录音失败：{}", msg),
            Self::CaptureFailed(msg) => format!("音频捕获错误：{}", msg),
            Self::EmptyRecording => "录音为空，请重试。".to_string(),
        }
    }

    /// 映射为前端可翻译的错误码。
    pub fn user_error(&self) -> UserFacingError {
        match self {
            Self::StartFailed(msg) => {
                UserFacingError::with("recorder.start_failed", "detail", msg.clone())
            }
            Self::StopFailed(msg) => {
                UserFacingError::with("recorder.stop_failed", "detail", msg.clone())
            }
            Self::CaptureFailed(msg) => {
                UserFacingError::with("recorder.capture_failed", "detail", msg.clone())
            }
            Self::EmptyRecording => UserFacingError::bare("recorder.empty_recording"),
        }
    }
}

/// 音频编解码错误。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AudioError {
    #[error("PCM data must not be empty")]
    EmptyPcm,

    #[error("sample rate must be positive")]
    InvalidSampleRate,

    #[error("channels must be positive")]
    InvalidChannels,

    #[error("bits per sample must be positive")]
    InvalidBitsPerSample,

    #[error("PCM data too large for WAV format (>4 GB)")]
    PcmTooLarge,

    #[error("WAV data too short")]
    WavTooShort,

    #[error("not a valid WAV file")]
    InvalidWavHeader,

    #[error("no data chunk found in WAV")]
    MissingDataChunk,
}

/// 输出（剪贴板）错误。
#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error("no clipboard tool found")]
    NoClipboardTool,

    #[error("clipboard error: {0}")]
    ClipboardFailed(String),
}

/// 按键监听器错误。
#[derive(Debug, thiserror::Error)]
pub enum KeyListenerError {
    #[error("key listener tool not found: {0}")]
    ToolNotFound(String),

    #[error("unsupported activation key: '{0}'")]
    UnsupportedKey(String),

    #[error("key listener start failed: {0}")]
    StartFailed(String),

    #[error("keycode resolution failed: {0}")]
    ResolveFailed(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// 模型管理错误。
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("unknown model: {0}")]
    UnknownModel(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("download failed: {0}")]
    DownloadFailed(String),

    #[error("HTTP error: {0}")]
    HttpError(String),
}

/// 配置错误。
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    ParseError(String),

    #[error("TOML serialize error: {0}")]
    SerializeError(String),

    #[error("validation failed:\n{0}")]
    ValidationFailed(String),
}

/// 历史记录存储错误。
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(String),

    #[error("history entry not found: {0}")]
    NotFound(String),

    #[error("history lock poisoned")]
    LockPoisoned,

    #[error("serialization error: {0}")]
    SerializeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fatal_error_classification() {
        let err = PipelineError::Fatal(FatalError::ModelNotFound {
            model: "sense-voice".to_string(),
            searched: vec![],
        });
        assert!(err.is_fatal());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_recoverable_error_classification() {
        let err = PipelineError::Recoverable(RecoverableError::EmptyTranscription);
        assert!(!err.is_fatal());
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_fatal_error_messages() {
        let err = PipelineError::Fatal(FatalError::ModelNotFound {
            model: "sense-voice".to_string(),
            searched: vec![PathBuf::from("/models")],
        });
        let msg = err.message();
        assert!(msg.contains("本地模型未找到"));
        assert!(msg.contains("sense-voice"));
    }

    #[test]
    fn test_transcriber_error_messages() {
        let err = TranscriberError::EmptyAudio;
        assert!(err.message().contains("音频数据为空"));
    }

    #[test]
    fn test_polisher_error_messages() {
        let err = PolisherError::RateLimited;
        assert!(err.message().contains("频率受限"));
    }

    #[test]
    fn test_user_error_fatal_codes() {
        let ue = FatalError::ModelNotFound {
            model: "sense-voice".to_string(),
            searched: vec![PathBuf::from("/models/a"), PathBuf::from("/models/b")],
        }
        .user_error();
        assert_eq!(ue.code, "fatal.model_not_found");
        let params = ue.params.unwrap();
        assert_eq!(params.get("model").unwrap(), "sense-voice");
        assert_eq!(params.get("searched").unwrap(), "/models/a\n/models/b");

        let ue = FatalError::ApiAuthFailed {
            service: "MiMo",
            status: 401,
        }
        .user_error();
        assert_eq!(ue.code, "fatal.api_auth_failed");
        let params = ue.params.unwrap();
        assert_eq!(params.get("service").unwrap(), "MiMo");
        assert_eq!(params.get("status").unwrap(), "401");

        let ue = FatalError::KeyListenerFailed {
            backend: "xinput".to_string(),
            reason: "boom".to_string(),
        }
        .user_error();
        assert_eq!(ue.code, "fatal.key_listener_failed");
        let params = ue.params.unwrap();
        assert_eq!(params.get("backend").unwrap(), "xinput");
        assert_eq!(params.get("reason").unwrap(), "boom");

        // 包装变体透传内层错误码。
        let ue = FatalError::TranscriberInitFailed(TranscriberError::EmptyAudio).user_error();
        assert_eq!(ue.code, "transcriber.empty_audio");
    }

    #[test]
    fn test_user_error_recoverable_codes() {
        let ue = RecoverableError::PolishingFailed(PolisherError::RateLimited).user_error();
        assert_eq!(ue.code, "polisher.rate_limited");
        assert!(ue.params.is_none());

        assert_eq!(
            RecoverableError::EmptyTranscription.user_error(),
            UserFacingError::bare("transcription.empty")
        );
    }

    #[test]
    fn test_user_error_domain_codes() {
        let ue = TranscriberError::ApiError {
            status: 429,
            body: "busy".to_string(),
        }
        .user_error();
        assert_eq!(ue.code, "transcriber.api_error");
        let params = ue.params.unwrap();
        assert_eq!(params.get("status").unwrap(), "429");
        assert_eq!(params.get("body").unwrap(), "busy");

        let ue = TranscriberError::WavDecodeFailed("bad header".to_string()).user_error();
        assert_eq!(ue.code, "transcriber.wav_decode_failed");
        assert_eq!(ue.params.unwrap().get("reason").unwrap(), "bad header");

        assert_eq!(
            PolisherError::RetriesExhausted.user_error(),
            UserFacingError::bare("polisher.retries_exhausted")
        );

        let ue = RecorderError::StartFailed("no device".to_string()).user_error();
        assert_eq!(ue.code, "recorder.start_failed");
        assert_eq!(ue.params.unwrap().get("detail").unwrap(), "no device");

        assert_eq!(
            RecorderError::EmptyRecording.user_error(),
            UserFacingError::bare("recorder.empty_recording")
        );
    }

    #[test]
    fn test_user_error_pipeline_dispatch_and_serialization() {
        let ue = PipelineError::Fatal(FatalError::ApiAuthFailed {
            service: "LLM",
            status: 403,
        })
        .user_error();
        assert_eq!(ue.code, "fatal.api_auth_failed");

        let ue = PipelineError::Recoverable(RecoverableError::EmptyTranscription).user_error();
        assert_eq!(ue.code, "transcription.empty");

        // 无参错误码序列化时不携带 params 字段，有参时为 camelCase 的 params 对象。
        assert_eq!(
            serde_json::to_string(&PolisherError::RateLimited.user_error()).unwrap(),
            r#"{"code":"polisher.rate_limited"}"#
        );
        assert_eq!(
            serde_json::to_string(&TranscriberError::JsonError("bad".to_string()).user_error())
                .unwrap(),
            r#"{"code":"transcriber.json_error","params":{"detail":"bad"}}"#
        );
    }
}
