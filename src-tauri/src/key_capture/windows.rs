//! Windows 激活键捕获（WH_KEYBOARD_LL）。
//!
//! 临时挂一个低级键盘钩子，阻塞等待下一次物理按键。

use std::sync::mpsc;
use std::time::Duration;

use super::{windows_vk_code_to_name, CaptureActivationResponse, CaptureOutcome, KeyCapture};
use crate::key_listener::windows::{spawn_ll_keyboard_hook, HookHandle};

/// Windows 平台激活键捕获器。
pub struct WindowsKeyCapture;

impl WindowsKeyCapture {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsKeyCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyCapture for WindowsKeyCapture {
    fn capture(&mut self, timeout: Duration) -> Result<CaptureOutcome, String> {
        let (tx, rx) = mpsc::sync_channel::<u16>(1);
        let hook: HookHandle = spawn_ll_keyboard_hook(move |vk, pressed| {
            if pressed && tx.send(vk).is_ok() {
                // 收到第一个按键即结束钩子线程。
                unsafe {
                    let tid = windows::Win32::System::Threading::GetCurrentThreadId();
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                        tid,
                        windows::Win32::UI::WindowsAndMessaging::WM_QUIT,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
        })
        .map_err(|e| format!("键盘钩子启动失败: {e}"))?;

        let pressed = rx.recv_timeout(timeout).ok();

        drop(hook); // 确保 WM_QUIT 后钩子线程被回收

        Ok(match pressed {
            Some(vk) => CaptureOutcome::Captured(CaptureActivationResponse {
                key_name: windows_vk_code_to_name(vk),
                linux_evdev_code: None,
                windows_vk_code: Some(vk),
            }),
            None => CaptureOutcome::TimedOut,
        })
    }
}

/// 监听器实际用于匹配的 VK 码：优先“按下以设置”捕获到的值，否则由键名解析。
///
/// 自检比对与监听器过滤共用这一个函数，避免两处各解析一遍导致漂移。
pub fn configured_match_code(key_name: &str, captured_code: Option<u16>) -> Option<u16> {
    captured_code.or_else(|| super::key_name_to_windows_vk(key_name))
}
