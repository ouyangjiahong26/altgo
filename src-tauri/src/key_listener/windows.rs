//! Windows 按键监听器。
//!
//! 使用 `WH_KEYBOARD_LL` 低级键盘钩子在专属线程上接收全局按键事件，
//! 无轮询、无子进程。钩子回调只做 VK 匹配与通道发送，保证快速返回
//! （`LowLevelHooksTimeout` 超时会导致系统摘除钩子）。
//!
//! Windows key listener.
//!
//! Receives global key events on a dedicated thread through a `WH_KEYBOARD_LL` low-level hook—no
//! polling, no subprocesses. The hook callback only matches VKs and sends down a channel, keeping
//! returns fast (a `LowLevelHooksTimeout` overrun makes the system strip the hook).

use super::{KeyEvent, KeyListener};
use crate::config::KeyListenerConfig;
use crate::error::KeyListenerError;
use std::thread::JoinHandle;
use tokio::sync::mpsc;

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

/// 钩子线程回调：`(vk_code, pressed)`，仅物理按键（过滤注入事件）。
/// Hook-thread callback type: `(vk_code, pressed)`, physical keys only (injected events filtered).
pub(crate) type HookCallback = Box<dyn Fn(u16, bool) + Send>;

thread_local! {
    static HOOK_CALLBACK: std::cell::RefCell<Option<HookCallback>> =
        const { std::cell::RefCell::new(None) };
}

unsafe extern "system" fn ll_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let msg = w_param.0 as u32;
        let pressed = match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => {
                return CallNextHookEx(None, n_code, w_param, l_param);
            }
        };
        let info = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
        // 忽略 SendInput 等注入的合成事件，避免自动输入文本时自我反馈。
        // Ignore synthetic events injected via SendInput etc., so auto-typed text doesn't feed back.
        if (info.flags.0 & LLKHF_INJECTED.0) == 0 {
            HOOK_CALLBACK.with(|cb| {
                if let Some(cb) = cb.borrow().as_ref() {
                    cb(info.vkCode as u16, pressed);
                }
            });
        }
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}

/// 运行中的低级键盘钩子句柄；drop 时停止钩子线程。
/// Handle of a running low-level keyboard hook; dropping it stops the hook thread.
pub(crate) struct HookHandle {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl HookHandle {
    /// 请求钩子线程退出（post WM_QUIT）并回收线程。
    /// Asks the hook thread to quit (posts WM_QUIT) and joins it.
    pub fn stop(mut self) {
        self.stop_impl();
    }

    fn stop_impl(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        self.stop_impl();
    }
}

/// 在新线程上安装 WH_KEYBOARD_LL 钩子并运行消息循环。
///
/// `on_event` 在钩子线程上被调用，只应做极轻量的转发工作。
/// Installs the WH_KEYBOARD_LL hook on a new thread and runs its message loop.
///
/// `on_event` runs on the hook thread and must stay extremely lightweight forwarding work.
pub(crate) fn spawn_ll_keyboard_hook<F>(on_event: F) -> Result<HookHandle, String>
where
    F: Fn(u16, bool) + Send + 'static,
{
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
    let thread = std::thread::Builder::new()
        .name("ll-keyboard-hook".into())
        .spawn(move || {
            HOOK_CALLBACK.with(|cb| *cb.borrow_mut() = Some(Box::new(on_event)));
            let thread_id = unsafe { GetCurrentThreadId() };

            unsafe {
                let hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(ll_keyboard_proc),
                    HINSTANCE::default(),
                    0,
                )
                .map_err(|err| format!("SetWindowsHookExW failed: {err}"));
                let hook = match hook {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                if ready_tx.send(Ok(thread_id)).is_err() {
                    // 调用方已放弃，直接清理。
                    // Caller gave up; clean up right away.
                    let _ = UnhookWindowsHookEx(hook);
                    return;
                }

                let mut msg = Default::default();
                // GetMessageW 返回 0 表示收到 WM_QUIT，-1 表示错误。
                // GetMessageW returning 0 means WM_QUIT received; -1 means error.
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
                let _ = UnhookWindowsHookEx(hook);
            }
        })
        .map_err(|e| format!("failed to spawn hook thread: {e}"))?;

    let thread_id = ready_rx
        .recv()
        .map_err(|_| "hook thread exited before ready".to_string())??;

    Ok(HookHandle {
        thread_id,
        thread: Some(thread),
    })
}

/// WH_KEYBOARD_LL 键盘监听器。
/// The WH_KEYBOARD_LL key listener.
pub struct WindowsKeyListener {
    vk_code: u16,
    key_name: String,
    hook: Option<HookHandle>,
}

impl WindowsKeyListener {
    pub fn new(cfg: &KeyListenerConfig) -> Result<Self, KeyListenerError> {
        let vk_code = cfg
            .windows_vk_code
            .or_else(|| key_name_to_windows_vk_checked(&cfg.key_name))
            .ok_or_else(|| {
                KeyListenerError::UnsupportedKey(format!(
                    "无法将按键 '{}' 解析为 Windows VK 码，请通过“按下以设置”重新捕获",
                    cfg.key_name
                ))
            })?;
        Ok(Self {
            vk_code,
            key_name: cfg.key_name.clone(),
            hook: None,
        })
    }
}

fn key_name_to_windows_vk_checked(name: &str) -> Option<u16> {
    crate::key_capture::key_name_to_windows_vk(name)
}

impl KeyListener for WindowsKeyListener {
    fn start(
        &mut self,
    ) -> Result<(mpsc::UnboundedReceiver<KeyEvent>, &'static str), KeyListenerError> {
        if self.hook.is_some() {
            return Err(KeyListenerError::StartFailed(
                "listener already started".to_string(),
            ));
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let vk = self.vk_code;
        let hook = spawn_ll_keyboard_hook(move |event_vk, pressed| {
            if event_vk == vk {
                let _ = tx.send(KeyEvent { pressed });
            }
        })
        .map_err(KeyListenerError::StartFailed)?;

        tracing::info!(
            key = %self.key_name,
            vk = format!("0x{:02X}", self.vk_code),
            "Windows keyboard hook installed"
        );
        self.hook = Some(hook);
        Ok((rx, "windows-hook"))
    }
}

impl Drop for WindowsKeyListener {
    fn drop(&mut self) {
        if let Some(hook) = self.hook.take() {
            hook.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> crate::config::KeyListenerConfig {
        crate::config::KeyListenerConfig {
            key_name: "Alt_R".to_string(),
            linux_evdev_code: None,
            windows_vk_code: None,
            long_press_threshold: std::time::Duration::from_millis(400),
            double_click_interval: std::time::Duration::from_millis(200),
            min_press_duration: std::time::Duration::from_millis(80),
        }
    }

    #[test]
    fn new_resolves_default_key_to_rmenu() {
        let listener = WindowsKeyListener::new(&test_config()).unwrap();
        assert_eq!(listener.vk_code, crate::key_capture::windows_vk::VK_RMENU);
    }

    #[test]
    fn new_prefers_captured_vk_over_key_name() {
        let mut cfg = test_config();
        cfg.windows_vk_code = Some(0x29); // 左 Graves
        let listener = WindowsKeyListener::new(&cfg).unwrap();
        assert_eq!(listener.vk_code, 0x29);
    }

    #[test]
    fn new_rejects_unresolvable_key_name() {
        let mut cfg = test_config();
        cfg.key_name = "definitely-not-a-key".to_string();
        let err = match WindowsKeyListener::new(&cfg) {
            Err(e) => e,
            Ok(_) => panic!("expected UnsupportedKey"),
        };
        assert!(matches!(err, KeyListenerError::UnsupportedKey(_)));
    }
}
