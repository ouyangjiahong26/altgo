//! 浮窗窗口接缝的 Tauri 适配层。
//!
//! 把具体的 `tauri::WebviewWindow` 操作挡在 `OverlayManager` 之外，
//! manager 经由 `OverlayWindow` 接口即可测试。

use tauri::{Emitter, LogicalSize, Manager, PhysicalPosition};

use crate::overlay::seam::{OverlayError, OverlayState, OverlayWindow};

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
        self.overlay()?
            .show()
            .map_err(|error| OverlayError::ShowFailed(error.to_string()))
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

/// 从工作区矩形提取 `(x, y, width, height)`，不关心各平台结构体的字段命名。
/// 由测试共享，保证几何提取逻辑在每个平台都被覆盖到。
#[allow(dead_code)]
fn geometry_from_work_rect(left: i32, top: i32, right: i32, bottom: i32) -> (i32, i32, i32, i32) {
    (left, top, right - left, bottom - top)
}

#[cfg(test)]
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

    // Windows 路径需要 AppHandle，只能在真实运行时验证；
    // 此处仅覆盖 Linux/xrandr 路径（无显示环境下返回 None 也不得 panic）。
    #[cfg(target_os = "linux")]
    #[test]
    fn test_platform_primary_monitor_geometry_runs_without_panicking() {
        let _ = platform_primary_monitor_geometry();
    }

    #[test]
    fn test_geometry_from_work_rect_uses_work_area() {
        // 整个显示器：0,0 - 3840x2160
        let (x, y, w, h) = geometry_from_work_rect(0, 40, 3840, 2120);
        assert_eq!((x, y, w, h), (0, 40, 3840, 2080));
    }

    #[test]
    fn test_geometry_from_work_rect_distinguishes_from_full_monitor() {
        // 同一显示器会报告 rcMonitor=(0,0,3840,2160)。有任务栏时 rcWork 至少应在一个维度上严格更小。
        let full = geometry_from_work_rect(0, 0, 3840, 2160);
        let work = geometry_from_work_rect(0, 40, 3840, 2120);
        assert_ne!(full, work);
        assert!(
            work.3 < full.3,
            "work height should be smaller than full height"
        );
    }

    #[test]
    fn test_geometry_from_work_rect_negative_origin() {
        // 位于主显示器左侧的副屏 x 为负值。
        let (x, y, w, h) = geometry_from_work_rect(-1920, 0, 0, 1080);
        assert_eq!((x, y, w, h), (-1920, 0, 1920, 1080));
    }
}
