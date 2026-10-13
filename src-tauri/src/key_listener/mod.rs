//! 按键监听器模块。
//!
//! Linux：`xinput test-xi2`（XInput2 扩展）或 `evtest` 回退。
//! Windows：`WH_KEYBOARD_LL` 低级键盘钩子（`windows.rs`）。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
pub(crate) mod windows;

#[cfg(target_os = "linux")]
pub use linux::{list_keyboard_devices, X11Listener};
/// xinput 路径的端到端测试需要真实 X 服务器与外部工具，单列一个文件。
#[cfg(all(test, target_os = "linux"))]
mod linux_x11_e2e;
#[cfg(target_os = "windows")]
pub use windows::WindowsKeyListener;

#[cfg(target_os = "linux")]
pub type PlatformListener = X11Listener;
#[cfg(target_os = "windows")]
pub type PlatformListener = WindowsKeyListener;

/// 按键事件。
#[derive(Debug)]
pub struct KeyEvent {
    /// 是否为按下事件
    pub pressed: bool,
}

/// 自检等原始按键观察的等待窗口。
pub const PROBE_WINDOW: std::time::Duration = std::time::Duration::from_secs(8);

/// 监听器是否正在运行。
///
/// 只有监听器在跑时原始按键旁路才有人喂；“按下以设置”据此决定走旁路还是退回临时钩子。
static LISTENER_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 流水线在监听器启动成功后置位、主循环收尾时清除。
pub(crate) fn set_listener_active(active: bool) {
    LISTENER_ACTIVE.store(active, std::sync::atomic::Ordering::SeqCst);
}

/// 监听器是否正在运行。
pub fn is_listener_active() -> bool {
    LISTENER_ACTIVE.load(std::sync::atomic::Ordering::SeqCst)
}

/// 监听器原始按键的旁路通道。
///
/// 监听器按配置过滤后才发 [`KeyEvent`]，过滤掉的那些在管道里看不到；自检要看的是
/// “按键到底有没有到达监听器”，所以需要一条保留过滤前全部按键的旁路。放在进程级：
/// 应用是单实例，任一时刻只有一个监听器在跑，自检也不会并发，单槽够用。
type ActivationKeySender =
    tokio::sync::mpsc::UnboundedSender<crate::key_capture::CaptureActivationResponse>;

static ACTIVATION_KEY_TAP: std::sync::LazyLock<std::sync::Mutex<Option<ActivationKeySender>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

/// 原始按键订阅：drop 时自行注销，避免把已无接收者的通道留在槽里。
pub struct ActivationKeySubscription {
    rx: tokio::sync::mpsc::UnboundedReceiver<crate::key_capture::CaptureActivationResponse>,
}

impl ActivationKeySubscription {
    /// 取下一个原始按键，窗口内没有按键则返回 `None`。
    pub async fn next(
        &mut self,
        window: std::time::Duration,
    ) -> Option<crate::key_capture::CaptureActivationResponse> {
        tokio::time::timeout(window, self.rx.recv())
            .await
            .ok()
            .flatten()
    }

    /// 取下一个物理按键，跳过其他程序注入的合成事件；窗口内没有可用按键则返回 `None`。
    ///
    /// “按下以设置”要的是能真正触发起录的键，注入事件一律不算，与旧的自装钩子路径一致。
    /// 自检报的是“按键有没有到达”，注入的也要看得见，所以只有捕获用这个入口。
    pub async fn next_physical(
        &mut self,
        window: std::time::Duration,
    ) -> Option<crate::key_capture::CaptureActivationResponse> {
        let deadline = tokio::time::Instant::now() + window;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match self.next(remaining).await {
                Some(observation) if observation.injected != Some(true) => {
                    return Some(observation)
                }
                Some(_) => continue,
                None => return None,
            }
        }
    }
}

impl Drop for ActivationKeySubscription {
    fn drop(&mut self) {
        let mut slot = ACTIVATION_KEY_TAP.lock().unwrap_or_else(|e| e.into_inner());
        *slot = None;
    }
}

/// 订阅监听器收到的原始按键。已有订阅者时旧订阅被顶掉，其接收端随即失效。
pub fn subscribe_activation_keys() -> ActivationKeySubscription {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut slot = ACTIVATION_KEY_TAP.lock().unwrap_or_else(|e| e.into_inner());
    *slot = Some(tx);
    ActivationKeySubscription { rx }
}

/// 监听器每收到一个原始按键就调它一次；没有订阅者时什么也不做。
pub(crate) fn publish_activation_key(observation: crate::key_capture::CaptureActivationResponse) {
    let slot = ACTIVATION_KEY_TAP.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(tx) = slot.as_ref() {
        let _ = tx.send(observation);
    }
}

use crate::error::KeyListenerError;

/// 持续监听触发键的 trait seam。
///
/// 由平台 adapter 实现。pipeline 以 `Box<dyn KeyListener>` 消费，便于注入测试 fake。
pub trait KeyListener: Send {
    /// 开始监听，返回事件通道与后端标识（如 `"xinput"`）。
    fn start(
        &mut self,
    ) -> Result<(tokio::sync::mpsc::UnboundedReceiver<KeyEvent>, &'static str), KeyListenerError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用 fake listener，验证 trait 可以被 boxing 并启动。
    struct FakeListener {
        backend: &'static str,
    }

    impl KeyListener for FakeListener {
        fn start(
            &mut self,
        ) -> Result<(tokio::sync::mpsc::UnboundedReceiver<KeyEvent>, &'static str), KeyListenerError>
        {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let _ = tx.send(KeyEvent { pressed: true });
            let _ = tx.send(KeyEvent { pressed: false });
            Ok((rx, self.backend))
        }
    }

    #[test]
    fn boxed_key_listener_starts_and_returns_events() {
        let mut listener: Box<dyn KeyListener> = Box::new(FakeListener {
            backend: "fake-test",
        });
        let (mut rx, name) = listener.start().unwrap();
        assert_eq!(name, "fake-test");

        let first = rx.try_recv().unwrap();
        assert!(first.pressed);
        let second = rx.try_recv().unwrap();
        assert!(!second.pressed);
    }

    #[test]
    fn key_event_debug_format() {
        let evt = KeyEvent { pressed: true };
        let dbg = format!("{:?}", evt);
        assert!(dbg.contains("KeyEvent"));
        assert!(dbg.contains("true"));
    }

    /// 两次观察只区分注入标记，其余字段与主题无关。
    fn observation(injected: bool) -> crate::key_capture::CaptureActivationResponse {
        crate::key_capture::CaptureActivationResponse {
            key_name: "Alt_R".to_string(),
            linux_evdev_code: None,
            windows_vk_code: Some(0xA5),
            injected: Some(injected),
        }
    }

    /// 捕获要的是能起录的物理键，注入的合成事件要跳过；只有注入事件时按超时收尾。
    ///
    /// 旁路槽是进程级单槽，两条断言合在一个用例里跑，避免并发订阅互相顶掉。
    #[tokio::test]
    async fn next_physical_skips_injected_and_times_out_without_one() {
        let mut subscription = subscribe_activation_keys();

        publish_activation_key(observation(true));
        publish_activation_key(observation(false));
        let captured = subscription
            .next_physical(std::time::Duration::from_secs(1))
            .await
            .expect("应当拿到物理按键");
        assert_eq!(captured.injected, Some(false));

        publish_activation_key(observation(true));
        assert!(subscription
            .next_physical(std::time::Duration::from_millis(50))
            .await
            .is_none());
    }
}
