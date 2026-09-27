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

use crate::error::KeyListenerError;

/// 持续监听激活键的 trait seam。
///
/// 由平台 adapter 实现；pipeline 以 `Box<dyn KeyListener>` 消费，便于注入测试 fake。
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
}
