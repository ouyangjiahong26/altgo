//! 管道事件接收器接口与共享类型。

use crate::error::UserFacingError;
use crate::pipeline_controller::PipelineStatus;

use super::pending::PendingRecordingInfo;

/// 单次转写回合的耗时与后端元数据，随结果一路流到历史落盘。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionMetrics {
    /// 转写后端：`"local"`（本地 SenseVoice）或 `"online"`（在线 MiMo）。
    pub backend: String,
    /// 录音时长（毫秒）。
    pub recording_ms: u64,
    /// 转写耗时（毫秒）。
    pub transcribe_ms: u64,
    /// 润色耗时（毫秒），未启用润色（`none` 档）时为 `None`。
    pub polish_ms: Option<u64>,
}

impl Default for TranscriptionMetrics {
    fn default() -> Self {
        Self {
            backend: "local".to_string(),
            recording_ms: 0,
            transcribe_ms: 0,
            polish_ms: None,
        }
    }
}

/// 转写结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TranscriptionResult {
    /// 处理后的文本（润色成功时为润色文本，否则为原始转写文本）
    pub text: String,
    /// 原始转写文本（润色前）
    pub raw_text: String,
    /// 润色是否失败
    pub polish_failed: bool,
    /// 润色失败时的错误码（经事件通道传给前端字典翻译）
    #[serde(default)]
    pub polish_error: Option<UserFacingError>,
    /// 本次转写回合的耗时与后端元数据（历史展示用）。
    pub metrics: TranscriptionMetrics,
}

/// 管道事件接收器。
///
/// 所有方法均为同步，实现方内部处理异步操作（如 `tokio::spawn`）。
/// 实现方必须是 `Send + Sync + 'static`，以支持跨线程使用。
pub trait PipelineSink: Send + Sync + 'static {
    /// 管道状态变化（idle / recording / processing / done / stopped）。
    fn on_status_change(&self, status: PipelineStatus);

    /// 管道错误（结构化错误码，由前端字典翻译）。
    fn on_error(&self, error: &UserFacingError);

    /// 转写+润色完成，输出结果。
    fn on_transcription_result(&self, output: &TranscriptionResult);

    /// 转写/润色进度更新。`phase` 为 `"transcribe"` / `"polish"` / `"done"`，
    /// `fraction` 为 0–1 或 `None`（不确定进度）。
    fn on_progress(&self, phase: &str, fraction: Option<f32>);

    /// 按键监听后端已启动（如 `"xinput"` / `"evtest"`）。
    fn on_key_listener_backend(&self, backend: &str);

    /// 实时感知音频电平更新（0.0 ~ 1.0）。
    fn on_audio_level(&self, _level: f32) {}

    /// 待重试录音槽位变化：`Some` = 转写失败后保留了录音本体，`None` = 槽位
    /// 已清空（重试成功或用户放弃）。默认空实现，供不需要该事件的实现省略。
    fn on_pending_recording(&self, _pending: Option<&PendingRecordingInfo>) {}
}

/// 派发结果。
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    /// 已写入剪贴板、待展示的文本。
    pub text: String,
    /// 历史是否追加成功。
    pub history_appended: bool,
}
