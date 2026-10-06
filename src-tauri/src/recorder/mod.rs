//! 录音模块。
//!
//! Linux：`PulseRecorder`（`parecord`）。Windows：`WindowsRecorder`（cpal / WASAPI）。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::PulseRecorder;
#[cfg(target_os = "windows")]
pub use windows::WindowsRecorder;

#[cfg(target_os = "linux")]
pub type PlatformRecorder = PulseRecorder;
#[cfg(target_os = "windows")]
pub type PlatformRecorder = WindowsRecorder;

use std::sync::Arc;

pub use crate::error::RecorderError;

/// SenseVoice 的固定输入采样率（Hz）。
pub const SAMPLE_RATE: u32 = 16_000;

/// 实时音频电平回调函数类型。
pub type AudioLevelCallback = Arc<dyn Fn(f32) + Send + Sync>;

pub trait Recorder: Send {
    fn start_recording(&mut self) -> Result<(), RecorderError>;
    fn stop_recording(&self) -> Result<Vec<u8>, RecorderError>;
    fn is_recording(&self) -> bool;
    /// 注册录音时的实时音频电平回调。
    fn set_audio_level_callback(&mut self, _callback: Option<AudioLevelCallback>) {}
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{PulseRecorder, SAMPLE_RATE};
    use crate::error::RecorderError;
    use crate::recorder::Recorder;

    /// 验证类型化错误变体能穿透 trait 边界：录音器必须直接交还 `RecorderError`，
    /// 任何在这里包一层 `anyhow` 的写法都是回归。
    #[test]
    fn pulse_recorder_returns_typed_recorder_error_on_empty_stop() {
        let rec = PulseRecorder::new(SAMPLE_RATE);
        // 未启动录音线程时，stop_recording 不会 join 任何东西，
        // 缓冲区为空，返回 EmptyRecording。
        let err = rec.stop_recording().unwrap_err();
        assert!(matches!(err, RecorderError::EmptyRecording));
    }
}
