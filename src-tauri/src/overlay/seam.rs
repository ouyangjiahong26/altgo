//! 悬浮窗窗口接缝。
//!
//! 定义 `OverlayWindow` 接口：`OverlayManager` 与窗口系统交互的唯一通道。平台相关的
//! Tauri 调用收进真正的 seam 后面，manager 的行为即可用 fake adapter 测试。

use tauri::{LogicalSize, PhysicalPosition};
use thiserror::Error;

/// 悬浮窗状态管理 seam。
///
/// 由 `OverlayManager` 实现，`TauriPipelineSink` 以 `Box<dyn OverlaySink>` 的形式消费，
/// sink 因此不必依赖具体的 manager 或窗口类型。
pub trait OverlaySink: Send + Sync {
    fn set_state(&self, state: OverlayState);
}

/// 驱动悬浮窗过程中可能出现的错误。
#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("overlay window not found")]
    WindowNotFound,

    #[error("failed to emit overlay state: {0}")]
    EmitFailed(String),

    #[error("failed to set size: {0}")]
    SetSizeFailed(String),

    #[error("failed to set position: {0}")]
    SetPositionFailed(String),

    #[error("failed to show window: {0}")]
    ShowFailed(String),

    #[error("failed to hide window: {0}")]
    HideFailed(String),

    #[error("failed to read scale factor: {0}")]
    ScaleFactorFailed(String),

    #[error("failed to query primary monitor: {0}")]
    PrimaryMonitorFailed(String),

    #[error("failed to prepare window for show: {0}")]
    PrepareForShowFailed(String),
}

/// Overlay 相位。Rust 内部以枚举流通，仅在序列化给前端时转成字符串：
/// `"recording"` / `"processing"` / `"done"` / `"hidden"`，
/// 与前端 `overlay-state` 事件协议保持一致。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPhase {
    Recording,
    Processing,
    Done,
    Hidden,
}

impl OverlayPhase {
    /// 与前端 `overlay-state` 协议一致的小写名称（与 serde 序列化相同）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Processing => "processing",
            Self::Done => "done",
            Self::Hidden => "hidden",
        }
    }
}

/// 从 Rust 发往前端悬浮窗的视觉状态。
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayState {
    /// 当前相位（序列化为前端协议字符串）。
    pub phase: OverlayPhase,
}

impl OverlayState {
    pub fn recording() -> Self {
        Self {
            phase: OverlayPhase::Recording,
        }
    }

    pub fn processing() -> Self {
        Self {
            phase: OverlayPhase::Processing,
        }
    }

    pub fn done() -> Self {
        Self {
            phase: OverlayPhase::Done,
        }
    }

    pub fn hidden() -> Self {
        Self {
            phase: OverlayPhase::Hidden,
        }
    }
}

/// `OverlayManager` 所需的窗口系统操作。
///
/// 所有方法都是同步的：底层 Tauri 窗口 API 本身同步，manager 也不需要异步控制流。
pub trait OverlayWindow: Send + Sync + Clone {
    /// 经 Tauri 事件（或等价通道）把视觉状态发给前端。
    fn emit_state(&self, state: &OverlayState) -> Result<(), OverlayError>;

    /// 把悬浮窗调整为指定的逻辑尺寸。
    fn set_size(&self, size: LogicalSize<f64>) -> Result<(), OverlayError>;

    /// 把悬浮窗移动到指定的物理位置。
    fn set_position(&self, position: PhysicalPosition<i32>) -> Result<(), OverlayError>;

    /// 在悬浮窗显示前准备原生窗口标志。
    fn prepare_for_show(&self) -> Result<(), OverlayError>;

    /// 显示悬浮窗。
    fn show(&self) -> Result<(), OverlayError>;

    /// 隐藏悬浮窗。
    fn hide(&self) -> Result<(), OverlayError>;

    /// 返回悬浮窗所在显示器的缩放系数。
    fn scale_factor(&self) -> Result<f64, OverlayError>;

    /// 以 `(x, y, width, height)` 物理像素形式返回主显示器几何信息。
    fn primary_monitor_geometry(&self) -> Result<(i32, i32, i32, i32), OverlayError>;
}

/// 悬浮窗在屏幕上的预设位置。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayPosition {
    /// 底部居中（默认）。
    #[default]
    BottomCenter,
    /// 顶部居中。
    TopCenter,
}

impl OverlayPosition {
    /// 从配置字符串解析，未知值回退到底部居中，保证旧配置与手误值可用。
    pub fn effective(value: &str) -> Self {
        match value {
            "top_center" => Self::TopCenter,
            _ => Self::BottomCenter,
        }
    }
}

#[cfg(test)]
mod position_tests {
    use super::OverlayPosition;

    #[test]
    fn effective_parses_known_values() {
        assert_eq!(
            OverlayPosition::effective("bottom_center"),
            OverlayPosition::BottomCenter
        );
        assert_eq!(
            OverlayPosition::effective("top_center"),
            OverlayPosition::TopCenter
        );
    }

    #[test]
    fn effective_falls_back_to_bottom_center_for_unknown_values() {
        assert_eq!(
            OverlayPosition::effective(""),
            OverlayPosition::BottomCenter
        );
        assert_eq!(
            OverlayPosition::effective("left"),
            OverlayPosition::BottomCenter
        );
    }
}
