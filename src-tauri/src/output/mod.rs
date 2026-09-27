//! 输出模块。
//!
//! Linux：`xclip`/`xsel`/`wl-copy` 写剪贴板。Windows：arboard 写剪贴板 + SendInput 注入文本。
//!
//! `Output` trait 将剪切板抽象为可替换 seam，使业务层不依赖平台细节。
//! 所有剪切板写入（包括 `cmd.rs::copy_text`）均通过 trait 路径完成。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

/// 平台 output adapter 类型别名。
#[cfg(target_os = "linux")]
pub type PlatformOutput = linux::LinuxOutput;
#[cfg(target_os = "windows")]
pub type PlatformOutput = windows::WindowsOutput;

use std::sync::Arc;

use crate::error::OutputError;

/// 剪切板写入的抽象接口。
///
/// 由 `voice_pipeline::process_transcription_result` 持有，使输出行为可在测试中替换。
pub trait Output: Send + Sync {
    /// 将文本写入系统剪切板。
    fn write_clipboard(&self, text: &str) -> Result<(), OutputError>;

    /// 将文本注入到当前焦点窗口（模拟键盘输入）。
    ///
    /// 仅 Windows 实现为 SendInput 注入；其他平台默认 no-op。
    fn inject_text(&self, _text: &str) -> Result<(), OutputError> {
        Ok(())
    }

    /// 支持 clone 为 trait object（用于 `PipelineEventHandler::Clone`）。
    fn clone_box(&self) -> Arc<dyn Output>;
}
