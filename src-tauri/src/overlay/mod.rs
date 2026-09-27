//! 悬浮窗模块。
//!
//! 把 Overlay 的三层职责拆成独立子模块，对齐项目已有的
//! `key_listener/`、`recorder/`、`output/` 目录组织：
//! - `seam`：`OverlayWindow` / `OverlaySink` / `OverlayState` / `OverlayError` 接口
//! - `manager`：`OverlayManager`，把状态意图翻译成窗口操作
//! - `tauri`：`TauriOverlayWindow`，Tauri 平台的 `OverlayWindow` adapter
//! - `activity`：`UserActivityClock`，自动淡出用的全局输入活动时钟

pub mod activity;
pub mod manager;
pub mod seam;
pub mod tauri;
