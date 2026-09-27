//! Windows 激活键捕获（WH_KEYBOARD_LL）。
//!
//! 临时挂一个低级键盘钩子，阻塞等待下一次物理按键。

use std::sync::mpsc;
use std::time::Duration;

use super::{windows_vk_code_to_name, CaptureActivationResponse, KeyCapture};
use crate::key_listener::windows::{spawn_ll_keyboard_hook, HookHandle};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(12);

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
    fn capture(&mut self) -> Result<CaptureActivationResponse, String> {
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

        let result = rx
            .recv_timeout(CAPTURE_TIMEOUT)
            .map_err(|_| "超时：未检测到按键".to_string());

        drop(hook); // 确保 WM_QUIT 后钩子线程被回收
        let vk = result?;
        Ok(CaptureActivationResponse {
            key_name: windows_vk_code_to_name(vk),
            linux_evdev_code: None,
            windows_vk_code: Some(vk),
        })
    }
}
