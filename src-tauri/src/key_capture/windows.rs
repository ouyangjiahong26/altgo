//! Windows 激活键捕获（WH_KEYBOARD_LL）。
//!
//! 临时挂一个低级键盘钩子，阻塞等待下一次物理按键。

use std::sync::mpsc;

use super::{
    windows_vk_code_to_name, CaptureActivationResponse, CaptureOptions, CaptureOutcome, KeyCapture,
};
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
    fn capture(&mut self, options: CaptureOptions) -> Result<CaptureOutcome, String> {
        let CaptureOptions {
            timeout,
            accept_injected,
            on_armed,
        } = options;
        let (tx, rx) = mpsc::sync_channel::<(u16, bool)>(1);
        let hook: HookHandle = spawn_ll_keyboard_hook(move |event| {
            if !event.pressed {
                return;
            }
            if event.injected && !accept_injected {
                return;
            }
            if tx.send((event.vk, event.injected)).is_ok() {
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

        // 钩子已挂上，此后按键才收得到。
        if let Some(on_armed) = on_armed {
            on_armed();
        }

        let pressed = rx.recv_timeout(timeout).ok();

        drop(hook); // 确保 WM_QUIT 后钩子线程被回收

        Ok(match pressed {
            Some((vk, injected)) => CaptureOutcome::Captured(CaptureActivationResponse {
                key_name: windows_vk_code_to_name(vk),
                linux_evdev_code: None,
                windows_vk_code: Some(vk),
                injected: Some(injected),
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

/// 超时未检测到按键时给用户的提示。
pub fn capture_timeout_message() -> &'static str {
    "超时：未检测到按键"
}

/// 当前配置下监听器实际用于匹配的 VK 码，供自检比对。
pub fn configured_match_code_from_config(cfg: &crate::config::KeyListenerConfig) -> Option<u16> {
    configured_match_code(&cfg.key_name, cfg.windows_vk_code)
}
