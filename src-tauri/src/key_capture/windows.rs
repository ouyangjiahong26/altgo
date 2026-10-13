//! Windows 触发键捕获（WH_KEYBOARD_LL）。
//!
//! 临时挂一个低级键盘钩子，阻塞等待下一次物理按键。

use std::sync::mpsc;
use std::time::Duration;

use super::{windows_vk_code_to_name, CaptureActivationResponse, CaptureOutcome, KeyCapture};
use crate::key_listener::windows::{spawn_ll_keyboard_hook, HookHandle};

/// Windows 平台触发键捕获器。
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
        let hook: HookHandle = spawn_ll_keyboard_hook(move |event| {
            // 注入的合成事件一律忽略：防止应用自动注入文本时自我反馈。
            if event.injected || !event.pressed {
                return;
            }
            if tx.send(event.vk).is_ok() {
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
                // “按下以设置”只认物理按键，走到这里的必然不是注入事件。
                injected: Some(false),
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

/// 监听没在跑、退回临时钩子时的超时提示。
///
/// 临时钩子是后装的，altgo 窗口获得焦点时可能收不到按键，把焦点移开就能用。
pub fn capture_fallback_timeout_message() -> &'static str {
    "超时：未检测到按键。当前用的是临时钩子，请先把焦点切到其他窗口再按一次。"
}

/// 当前配置下监听器实际用于匹配的 VK 码，供自检比对。
pub fn configured_match_code_from_config(cfg: &crate::config::KeyListenerConfig) -> Option<u16> {
    configured_match_code(&cfg.key_name, cfg.windows_vk_code)
}
