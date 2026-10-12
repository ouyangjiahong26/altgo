//! Windows 按键监听器。
//!
//! 使用 `WH_KEYBOARD_LL` 低级键盘钩子在专属线程上接收全局按键事件，
//! 无轮询、无子进程。钩子过程只把原始事件转给调用方回调，注入过滤与
//! VK 匹配都在回调里做，保证钩子过程快速返回
//! （`LowLevelHooksTimeout` 超时会导致系统摘除钩子）。

use super::{publish_activation_key, KeyEvent, KeyListener};
use crate::config::KeyListenerConfig;
use crate::error::KeyListenerError;
use std::thread::JoinHandle;
use tokio::sync::mpsc;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

/// 钩子线程收到的原始按键。
pub(crate) struct HookEvent {
    /// Windows 虚拟键码
    pub vk: u16,
    /// 是否为按下事件
    pub pressed: bool,
    /// 是否由其他程序注入（SendInput、改键工具、键盘厂商驱动、输入法）
    pub injected: bool,
}

/// 钩子线程回调。只做转发，注入过滤与键码匹配留给调用方，保证钩子过程快速返回。
pub(crate) type HookCallback = Box<dyn Fn(HookEvent) + Send>;

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
        let event = HookEvent {
            vk: info.vkCode as u16,
            pressed,
            injected: (info.flags.0 & LLKHF_INJECTED.0) != 0,
        };
        HOOK_CALLBACK.with(|cb| match cb.borrow().as_ref() {
            Some(callback) => callback(event),
            // 事件投到没有回调的线程上时，钩子看起来“装好了却收不到”，
            // 这条日志是区分它与“系统根本没调用”的唯一线索。
            None => tracing::debug!(
                thread_id = GetCurrentThreadId(),
                vk = event.vk,
                injected = event.injected,
                "keyboard hook event delivered to a thread with no callback"
            ),
        });
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}

/// 运行中的低级键盘钩子句柄，drop 时停止钩子线程。
pub(crate) struct HookHandle {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl HookHandle {
    /// 钩子线程 id，供日志与诊断定位。
    pub(crate) fn thread_id(&self) -> u32 {
        self.thread_id
    }

    /// 请求钩子线程退出（post WM_QUIT）并回收线程。
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
pub(crate) fn spawn_ll_keyboard_hook<F>(on_event: F) -> Result<HookHandle, String>
where
    F: Fn(HookEvent) + Send + 'static,
{
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
    let thread = std::thread::Builder::new()
        .name("ll-keyboard-hook".into())
        .spawn(move || {
            HOOK_CALLBACK.with(|cb| *cb.borrow_mut() = Some(Box::new(on_event)));
            let thread_id = unsafe { GetCurrentThreadId() };

            unsafe {
                let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), None, 0)
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
                    let _ = UnhookWindowsHookEx(hook);
                    return;
                }
                tracing::debug!(thread_id, "keyboard hook installed and pumping messages");

                let mut msg = Default::default();
                // GetMessageW 返回 0 表示收到 WM_QUIT，-1 表示错误。
                // 两种退出都要留下原因：钩子线程悄悄死掉时，钩子看起来还是装好的。
                loop {
                    let ret = GetMessageW(&mut msg, None, 0, 0).0;
                    if ret == 0 {
                        tracing::debug!(thread_id, "keyboard hook thread received WM_QUIT");
                        break;
                    }
                    if ret == -1 {
                        tracing::debug!(thread_id, "GetMessageW failed in keyboard hook thread");
                        break;
                    }
                }
                let _ = UnhookWindowsHookEx(hook);
                tracing::debug!(thread_id, "keyboard hook removed");
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
pub struct WindowsKeyListener {
    vk_code: u16,
    key_name: String,
    hook: Option<HookHandle>,
}

impl WindowsKeyListener {
    pub fn new(cfg: &KeyListenerConfig) -> Result<Self, KeyListenerError> {
        let vk_code = crate::key_capture::configured_match_code(&cfg.key_name, cfg.windows_vk_code)
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
        let hook = spawn_ll_keyboard_hook(move |event| {
            // 旁路先发：过滤掉的按键也要能被自检看到，用户按错了键才知道自己按的是哪个。
            publish_activation_key(crate::key_capture::CaptureActivationResponse {
                key_name: crate::key_capture::windows_vk_code_to_name(event.vk),
                linux_evdev_code: None,
                windows_vk_code: Some(event.vk),
                injected: Some(event.injected),
            });
            // 忽略 SendInput 等注入的合成事件，避免应用自动注入文本时自我反馈。
            if event.injected {
                if event.vk == vk {
                    // 改键工具、键盘厂商驱动与输入法注入的按键走这里。丢得无声无息时
                    // 用户看到的就是“按了没反应”，所以至少要留一行日志。
                    tracing::debug!(vk, "configured key arrived as injected input, ignored");
                }
                return;
            }
            if event.vk == vk {
                tracing::debug!(vk, pressed = event.pressed, "configured key received");
                let _ = tx.send(KeyEvent {
                    pressed: event.pressed,
                });
            }
        })
        .map_err(KeyListenerError::StartFailed)?;

        tracing::info!(
            key = self.key_name,
            vk = format!("0x{:02X}", self.vk_code),
            thread_id = hook.thread_id(),
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
