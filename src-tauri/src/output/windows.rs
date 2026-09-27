//! Windows 输出模块。
//!
//! 剪贴板通过 arboard 写入；文本注入通过 SendInput 的 Unicode 事件实现
//! （支持中日韩字符）。

use super::Output;
use crate::error::OutputError;
use std::sync::Arc;

/// Windows 输出适配器。
pub struct WindowsOutput;

impl WindowsOutput {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl Output for WindowsOutput {
    fn write_clipboard(&self, text: &str) -> Result<(), OutputError> {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| OutputError::ClipboardFailed(e.to_string()))?;
        clipboard
            .set_text(text.to_string())
            .map_err(|e| OutputError::ClipboardFailed(e.to_string()))
    }

    fn inject_text(&self, text: &str) -> Result<(), OutputError> {
        send_unicode_text(text)
    }

    fn clone_box(&self) -> Arc<dyn Output> {
        Arc::new(WindowsOutput)
    }
}

/// 通过 SendInput 发送 Unicode 文本到当前焦点窗口。
fn send_unicode_text(text: &str) -> Result<(), OutputError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
        VIRTUAL_KEY,
    };

    let inputs: Vec<INPUT> = text
        .chars()
        .flat_map(|c| {
            let down = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: c as u16,
                        dwFlags: KEYEVENTF_UNICODE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let up = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: c as u16,
                        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            [down, up]
        })
        .collect();

    unsafe {
        let result = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if result == 0 {
            return Err(OutputError::ClipboardFailed(
                "SendInput 注入文本失败".to_string(),
            ));
        }
    }
    Ok(())
}
