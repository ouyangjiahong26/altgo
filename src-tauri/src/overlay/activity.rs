//! 用户活动时钟 —— 浮窗自动淡出的空闲感知接缝。
//!
//! 回答“距上次全局用户输入（键盘/鼠标）过去了多久”。浮窗管理器在
//! done 阶段据此判断“用户是否已继续工作”：粘贴、切窗口、打字都伴随
//! 全局输入活动，是“结果已被使用”的代理信号。生产实现仅 Windows
//! （GetLastInputInfo）；Linux 无统一空闲 API，采用固定超时降级
//! （见 ADR 0006），不使用本接缝。
//!
//! Answers how long ago the last global user input (keyboard/mouse) happened. During the done
//! phase the overlay manager consults it to decide whether the user has moved on: pasting,
//! window switching, and typing all come with global input activity—a proxy signal that the
//! result has been consumed. The production implementation is Windows-only (`GetLastInputInfo`);
//! Linux lacks a unified idle API and degrades to a fixed timeout (see ADR 0006), bypassing this seam.

/// 用户活动时钟：返回距上次全局输入事件（键盘/鼠标）的毫秒数。
/// User activity clock: returns milliseconds since the last global input event (keyboard/mouse).
pub trait UserActivityClock: Send + Sync {
    fn ms_since_last_input(&self) -> u64;
}

/// Windows 实现：GetLastInputInfo + GetTickCount（同源 32 位 tick，
/// wrapping 处理约 49.7 天的回绕）。
/// Windows implementation: GetLastInputInfo + GetTickCount (same-origin 32-bit tick,
/// with wraparound handled across the ~49.7-day rollover).
#[cfg(target_os = "windows")]
pub struct WindowsActivityClock;

#[cfg(target_os = "windows")]
impl UserActivityClock for WindowsActivityClock {
    fn ms_since_last_input(&self) -> u64 {
        use windows::Win32::System::SystemInformation::GetTickCount;
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: `info` 按 API 约定初始化（cbSize 指示结构体大小）。
        // SAFETY: `info` is initialized per the API contract (cbSize declares the struct size).
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            return u64::MAX;
        }
        let now = unsafe { GetTickCount() };
        now.wrapping_sub(info.dwTime) as u64
    }
}
