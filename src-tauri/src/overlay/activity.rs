//! 用户活动时钟：悬浮窗自动淡出的空闲感知接缝。
//!
//! 回答“距上次全局用户输入（键盘/鼠标）过去了多久”。悬浮窗管理器在
//! done 相位据此判断“用户是否已继续工作”：粘贴、切窗口、打字都伴随
//! 全局输入活动，是“结果已被使用”的代理信号。生产实现仅 Windows
//! （GetLastInputInfo）。Linux 无统一空闲 API，采用固定超时降级
//! （见 ADR 0006），不使用本接缝。

/// 用户活动时钟：返回距上次全局输入事件（键盘/鼠标）的毫秒数。
pub trait UserActivityClock: Send + Sync {
    fn ms_since_last_input(&self) -> u64;
}

/// Windows 实现：GetLastInputInfo + GetTickCount（同源 32 位 tick，
/// wrapping 处理约 49.7 天的回绕）。
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
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            return u64::MAX;
        }
        let now = unsafe { GetTickCount() };
        now.wrapping_sub(info.dwTime) as u64
    }
}
