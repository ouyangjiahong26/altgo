//! 悬浮窗窗口接缝的 Tauri 适配层。
//!
//! 把具体的 `tauri::WebviewWindow` 操作挡在 `OverlayManager` 之外，
//! manager 经由 `OverlayWindow` 接口即可测试。

use tauri::{Emitter, LogicalSize, Manager, PhysicalPosition};

use crate::overlay::seam::{OverlayError, OverlayState, OverlayWindow, OVERLAY_SIZE};

const OVERLAY_WINDOW_LABEL: &str = "overlay";

/// `OverlayWindow` 的 Tauri 实现。
#[derive(Clone)]
pub struct TauriOverlayWindow {
    app: tauri::AppHandle,
}

impl TauriOverlayWindow {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }

    fn overlay(&self) -> Result<tauri::WebviewWindow, OverlayError> {
        self.app
            .get_webview_window(OVERLAY_WINDOW_LABEL)
            .ok_or(OverlayError::WindowNotFound)
    }

    /// 显示后校验窗口实际尺寸达到 [`OVERLAY_SIZE`]，被窗口系统钳制时打告警。
    ///
    /// Linux 的 WebKitGTK 给 WebView 强制最小内容尺寸（实测高 200 逻辑像素），
    /// 窗口创建时若被它撑大，后续 `set_size` 会静默失效且不报错；屏幕定位按
    /// 64 高推导，窗口实际偏高时贴底布局的胶囊会整体移出屏幕底边。窗口因此
    /// 保持 `resizable: true`（tauri.conf.json 的 overlay 配置）让 resize 生效，
    /// 此校验负责让钳制一旦复发就在日志里可见。
    fn warn_if_size_clamped(&self, overlay: &tauri::WebviewWindow) {
        let Ok(actual) = overlay.outer_size() else {
            return;
        };
        // Wayland 后端在窗口就绪前读到 0x0，不构成钳制证据，跳过告警。
        if actual.width == 0 || actual.height == 0 {
            return;
        }
        let Ok(scale) = overlay.scale_factor() else {
            return;
        };
        let expected = (
            (OVERLAY_SIZE.0 * scale).round() as i32,
            (OVERLAY_SIZE.1 * scale).round() as i32,
        );
        if !size_within_tolerance(expected, (actual.width as i32, actual.height as i32)) {
            tracing::warn!(
                expected = format!("{}x{}", expected.0, expected.1),
                actual = format!("{}x{}", actual.width, actual.height),
                "overlay window size was clamped by the windowing system"
            );
        }
    }
}

impl OverlayWindow for TauriOverlayWindow {
    fn emit_state(&self, state: &OverlayState) -> Result<(), OverlayError> {
        self.app
            .emit("overlay-state", state)
            .map_err(|error| OverlayError::EmitFailed(error.to_string()))
    }

    fn set_size(&self, size: LogicalSize<f64>) -> Result<(), OverlayError> {
        self.overlay()?
            .set_size(size)
            .map_err(|error| OverlayError::SetSizeFailed(error.to_string()))
    }

    fn set_position(&self, position: PhysicalPosition<i32>) -> Result<(), OverlayError> {
        self.overlay()?
            .set_position(position)
            .map_err(|error| OverlayError::SetPositionFailed(error.to_string()))
    }

    fn prepare_for_show(&self) -> Result<(), OverlayError> {
        let overlay = self.overlay()?;

        if let Err(error) = overlay.set_always_on_top(true) {
            tracing::warn!(error = %error, "overlay set_always_on_top failed");
        }
        if let Err(error) = overlay.set_skip_taskbar(true) {
            tracing::warn!(error = %error, "overlay set_skip_taskbar failed");
        }
        if let Err(error) = overlay.set_focusable(false) {
            tracing::warn!(error = %error, "overlay set_focusable failed");
        }
        if let Err(error) = overlay.set_shadow(false) {
            tracing::warn!(error = %error, "overlay set_shadow failed");
        }

        Ok(())
    }

    fn show(&self) -> Result<(), OverlayError> {
        let overlay = self.overlay()?;
        overlay
            .show()
            .map_err(|error| OverlayError::ShowFailed(error.to_string()))?;
        self.warn_if_size_clamped(&overlay);
        Ok(())
    }

    fn hide(&self) -> Result<(), OverlayError> {
        self.overlay()?
            .hide()
            .map_err(|error| OverlayError::HideFailed(error.to_string()))
    }

    fn scale_factor(&self) -> Result<f64, OverlayError> {
        self.overlay()?
            .scale_factor()
            .map_err(|error| OverlayError::ScaleFactorFailed(error.to_string()))
    }

    fn primary_monitor_geometry(&self) -> Result<(i32, i32, i32, i32), OverlayError> {
        #[cfg(target_os = "windows")]
        let geometry = platform_primary_monitor_geometry(&self.app);
        #[cfg(target_os = "linux")]
        let geometry = platform_primary_monitor_geometry();
        geometry.ok_or_else(|| {
            OverlayError::PrimaryMonitorFailed("no primary monitor available".into())
        })
    }
}

#[cfg(target_os = "windows")]
fn platform_primary_monitor_geometry(app: &tauri::AppHandle) -> Option<(i32, i32, i32, i32)> {
    let monitor = app.primary_monitor().ok()??;
    let position = monitor.position();
    let size = monitor.size();
    Some((
        position.x,
        position.y,
        size.width as i32,
        size.height as i32,
    ))
}

#[cfg(target_os = "linux")]
fn platform_primary_monitor_geometry() -> Option<(i32, i32, i32, i32)> {
    xrandr_primary_monitor()
}

/// 使用 `xrandr` 获取主显示器的物理像素几何。
#[cfg(target_os = "linux")]
fn xrandr_primary_monitor() -> Option<(i32, i32, i32, i32)> {
    let output = std::process::Command::new("xrandr").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let monitors = parse_xrandr_geometry(&text);
    monitors
        .iter()
        .find(|monitor| monitor.4)
        .map(|monitor| (monitor.0, monitor.1, monitor.2, monitor.3))
        .or_else(|| {
            monitors
                .into_iter()
                .next()
                .map(|monitor| (monitor.0, monitor.1, monitor.2, monitor.3))
        })
}

#[cfg(target_os = "linux")]
fn parse_xrandr_geometry(output: &str) -> Vec<(i32, i32, i32, i32, bool)> {
    let mut monitors = Vec::new();
    for line in output.lines() {
        if !line.contains(" connected ") {
            continue;
        }
        let is_primary = line.contains(" connected primary ");
        let after_conn = if is_primary {
            line.split(" connected primary ").nth(1)
        } else {
            line.split(" connected ").nth(1)
        };
        let after_conn = match after_conn {
            Some(value) => value,
            None => continue,
        };
        let end = after_conn.find(' ').unwrap_or(after_conn.len());
        let geometry = &after_conn[..end];

        let Some(x_idx) = geometry.find('x') else {
            continue;
        };
        let Some(plus1) = geometry.find('+') else {
            continue;
        };
        let Some(plus2) = geometry.rfind('+') else {
            continue;
        };
        if plus1 == plus2 {
            continue;
        }

        let width = geometry[..x_idx].parse::<i32>().unwrap_or(0);
        let height = geometry[x_idx + 1..plus1].parse::<i32>().unwrap_or(0);
        let x = geometry[plus1 + 1..plus2].parse::<i32>().unwrap_or(0);
        let y = geometry[plus2 + 1..].parse::<i32>().unwrap_or(0);

        if width > 0 && height > 0 {
            monitors.push((x, y, width, height, is_primary));
        }
    }
    monitors
}

/// 读回校验的容差：物理像素级的舍入差不算钳制。
const SIZE_TOLERANCE: i32 = 2;

/// 判断实际物理尺寸是否在期望尺寸的容差内。
fn size_within_tolerance(expected: (i32, i32), actual: (i32, i32)) -> bool {
    (expected.0 - actual.0).abs() <= SIZE_TOLERANCE
        && (expected.1 - actual.1).abs() <= SIZE_TOLERANCE
}

/// 这些用例只覆盖 Linux 的 xrandr 解析路径，Windows 路径需要 AppHandle、只能在
/// 真实运行时验证，所以整个模块按平台门控：否则 Windows 上会留下用不到的
/// `use super::*`，`clippy --all-targets` 会因此失败。
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_xrandr_geometry() {
        let sample = r#"
DP-1 connected primary 3840x2160+0+0 (normal left inverted right x axis y axis) 597mm x 336mm
DP-2 connected 1920x1080+3840+0 (normal left inverted right x axis y axis) 527mm x 296mm
"#;
        let parsed = parse_xrandr_geometry(sample);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], (0, 0, 3840, 2160, true));
        assert_eq!(parsed[1], (3840, 0, 1920, 1080, false));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_xrandr_skips_disconnected() {
        let sample = "DP-1 disconnected (normal left inverted right x axis y axis)";
        assert!(parse_xrandr_geometry(sample).is_empty());
    }

    // Windows 路径需要 AppHandle，只能在真实运行时验证。
    // 此处仅覆盖 Linux/xrandr 路径（无显示环境下返回 None 也不得 panic）。
    #[cfg(target_os = "linux")]
    #[test]
    fn test_platform_primary_monitor_geometry_runs_without_panicking() {
        let _ = platform_primary_monitor_geometry();
    }

    #[test]
    fn test_size_within_tolerance_accepts_matching_and_rounding() {
        assert!(size_within_tolerance((560, 128), (560, 128)));
        assert!(size_within_tolerance((560, 128), (562, 130)));
        assert!(size_within_tolerance((560, 128), (558, 126)));
    }

    #[test]
    fn test_size_within_tolerance_rejects_clamped_dimensions() {
        // WebKitGTK 最小内容尺寸把高度钳到 200 逻辑（2 倍缩放即 400 物理）。
        assert!(!size_within_tolerance((560, 128), (560, 400)));
        assert!(!size_within_tolerance((560, 128), (560, 124)));
    }
}
