//! 显示后端探测：决定在 GUI 初始化前是否需要切换 GDK 后端。
//!
//! Wayland 协议不允许客户端给自己的窗口定位，`set_position` 是 no-op，
//! 悬浮窗会被合成器摆到默认位置（GNOME 下为屏幕中央）。检测到 Wayland
//! 会话时切到 X11 后端（XWayland），悬浮窗定位即恢复生效。

/// 返回应设置的 `GDK_BACKEND` 值；`None` 表示保持现状。
///
/// - 非 Wayland 会话：不动。
/// - Wayland 会话且用户未显式设置 `GDK_BACKEND`：切到 `"x11"`（XWayland）。
/// - 用户显式设置过 `GDK_BACKEND`：尊重其选择，不覆盖。
pub fn resolve_display_backend(
    wayland_session: bool,
    explicit_gdk_backend: Option<&str>,
) -> Option<&'static str> {
    if wayland_session && explicit_gdk_backend.is_none() {
        Some("x11")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_session_is_left_untouched() {
        assert_eq!(resolve_display_backend(false, None), None);
    }

    #[test]
    fn wayland_session_without_explicit_backend_switches_to_x11() {
        assert_eq!(resolve_display_backend(true, None), Some("x11"));
    }

    #[test]
    fn wayland_session_respects_explicit_backend() {
        assert_eq!(resolve_display_backend(true, Some("wayland")), None);
        assert_eq!(resolve_display_backend(true, Some("x11")), None);
    }
}
