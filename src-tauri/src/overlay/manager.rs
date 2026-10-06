//! 悬浮窗管理模块。
//!
//! 把 Overlay 的状态意图与窗口物理操作分离：调用方只描述状态，本模块通过
//! `OverlayWindow` seam 计算尺寸和位置，再由具体 adapter 执行窗口操作。
//!
//! 与前端的分工：
//! - 本模块负责窗口物理层：emit `overlay-state` 事件、resize、reposition、show/hide
//! - 前端负责视觉层：CSS transition / animation 处理 entry / exit / crossfade

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use tauri::{LogicalSize, PhysicalPosition};

use crate::overlay::activity::UserActivityClock;
use crate::overlay::seam::{OverlayError, OverlaySink, OverlayWindow};

pub use crate::overlay::seam::{OverlayPhase, OverlayPosition, OverlayState};

/// 悬浮窗的固定逻辑尺寸（CSS pixels）。
///
/// 所有相位共用同一窗口尺寸（280px 窗口容纳单行 40px 胶囊，底部 12px + 顶部 12px 余量）。
/// 相位切换只改前端内容，不再触碰窗口几何：透明窗口 resize 时新暴露的区域
/// 在部分 Linux WM 上会合成出黑边，且窗口变形与前端 crossfade 错位会造成跳变。
const OVERLAY_SIZE: (f64, f64) = (280.0, 64.0);

/// 距屏幕边缘的偏移（CSS pixels），底部居中与顶部居中共用。
const EDGE_OFFSET: f64 = 80.0;

/// hidden 事件发出后、真正 hide 之前的延迟，给前端 exit 动画留出播放时间
/// （前端 --duration-normal 为 180 ms，再加少量余量）。
const HIDE_DELAY: Duration = Duration::from_millis(220);

/// 自动淡出生产参数：检测到输入活动后多久淡出（ADR 0006）。
/// 仅 Windows 空闲感知策略使用，Linux 走固定超时（见下）。
#[cfg(target_os = "windows")]
const AUTO_FADE_DELAY: Duration = Duration::from_secs(3);

/// 空闲感知的轮询间隔。
#[cfg(target_os = "windows")]
const ACTIVITY_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Linux 固定超时淡出时长（Wayland 无统一空闲 API 的降级策略，ADR 0006）。
#[cfg(target_os = "linux")]
const LINUX_FADE_TIMEOUT: Duration = Duration::from_secs(8);

/// done 悬浮窗的自动淡出策略。
#[derive(Clone)]
pub enum AutoFadePolicy {
    /// 空闲感知：done 出现即开始观察全局输入。结果出现时用户“最近刚操作过”
    /// （距上次输入不超过 fade_delay），或随后出现新输入，都视为用户已继续
    /// 工作，fade_delay 后淡出（倒计时不可逆），无输入则一直保留。
    ActivityAware {
        clock: Arc<dyn UserActivityClock>,
        poll_interval: Duration,
        fade_delay: Duration,
    },
    /// 固定超时：done 出现后固定时长淡出（无空闲检测平台的降级策略）。
    FixedTimeout { delay: Duration },
}

/// 按平台返回生产默认的自动淡出策略。
pub fn platform_auto_fade_policy() -> AutoFadePolicy {
    // Linux：Wayland 会话无统一的全局空闲查询 API，降级为固定超时。
    #[cfg(target_os = "linux")]
    {
        AutoFadePolicy::FixedTimeout {
            delay: LINUX_FADE_TIMEOUT,
        }
    }
    #[cfg(target_os = "windows")]
    {
        AutoFadePolicy::ActivityAware {
            clock: Arc::new(crate::overlay::activity::WindowsActivityClock),
            poll_interval: ACTIVITY_POLL_INTERVAL,
            fade_delay: AUTO_FADE_DELAY,
        }
    }
}

/// 悬浮窗管理器：负责把 Overlay 状态意图翻译成窗口操作。
#[derive(Clone)]
pub struct OverlayManager<W: OverlayWindow> {
    window: W,
    /// 悬浮窗的屏幕位置，构造时注入，配置变更经流水线重启后生效。
    position: OverlayPosition,
    /// 代际计数：每次 set_state 递增。延迟 hide 执行前比对代际，
    /// 防止“hide 延迟期间用户重新开始录音”时旧 hide 关掉新内容。
    generation: Arc<AtomicU64>,
    /// done 相位的自动淡出策略，`None` 时 done 相位一直保留。
    auto_fade: Option<Arc<AutoFadePolicy>>,
}

impl<W: OverlayWindow + 'static> OverlayManager<W> {
    pub fn new(window: W, position: OverlayPosition) -> Self {
        Self {
            window,
            position,
            generation: Arc::new(AtomicU64::new(0)),
            auto_fade: None,
        }
    }

    /// 注入自动淡出策略（builder）。
    pub fn with_auto_fade(mut self, policy: AutoFadePolicy) -> Self {
        self.auto_fade = Some(Arc::new(policy));
        self
    }

    /// 设置悬浮窗状态。
    ///
    /// 这是一个原子意图：调用方只需描述“现在应该显示什么相位”，
    /// 本方法内部一次性完成 resize → reposition → prepare → show → emit。
    /// 窗口尺寸是固定的，重复调用只是幂等的几何设置。
    pub fn set_state(&self, state: OverlayState) {
        let seq = self.generation.fetch_add(1, Ordering::SeqCst) + 1;

        if matches!(state.phase, OverlayPhase::Hidden) {
            self.enter_hidden(&state, seq);
            return;
        }

        self.apply_window_geometry();
        self.emit_and_show(&state);

        // done 相位挂起自动淡出观察：到点转 hidden（复用 generation 防竞态）。
        if matches!(state.phase, OverlayPhase::Done) {
            if let Some(policy) = &self.auto_fade {
                let manager = self.clone();
                let policy = Arc::clone(policy);
                std::thread::spawn(move || manager.watch_done(seq, &policy));
            }
        }
    }

    /// hidden 相位：先发状态给前端播退出动画，再延迟真正 hide。
    ///
    /// hide 放到独立线程延迟执行，执行前比对代际，防止“hide 延迟期间用户
    /// 重新开始录音”时旧 hide 关掉新内容。
    fn enter_hidden(&self, state: &OverlayState, seq: u64) {
        if let Err(error) = self.window.emit_state(state) {
            tracing::warn!(%error, "overlay state emit failed");
        }
        let window = self.window.clone();
        let generation = Arc::clone(&self.generation);
        std::thread::spawn(move || {
            std::thread::sleep(HIDE_DELAY);
            if generation.load(Ordering::SeqCst) != seq {
                return;
            }
            if let Err(error) = window.hide() {
                tracing::warn!(%error, "overlay hide failed");
            }
        });
    }

    /// 设置窗口尺寸与位置。窗口尺寸固定，重复调用只是幂等的几何设置。
    fn apply_window_geometry(&self) {
        let (width, height) = OVERLAY_SIZE;
        if let Err(error) = self.window.set_size(LogicalSize::new(width, height)) {
            tracing::warn!(%error, "overlay set_size failed");
        }

        match position_overlay(&self.window, width, height, self.position) {
            Ok(position) => {
                if let Err(error) = self.window.set_position(position) {
                    tracing::warn!(%error, "overlay set_position failed");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "overlay positioning failed");
            }
        }
    }

    /// 发状态给前端后准备并显示窗口。任一步失败只告警，不中断后续步骤。
    fn emit_and_show(&self, state: &OverlayState) {
        if let Err(error) = self.window.emit_state(state) {
            tracing::warn!(%error, "overlay state emit failed");
        }

        if let Err(error) = self.window.prepare_for_show() {
            tracing::warn!(%error, "overlay prepare_for_show failed");
        }

        if let Err(error) = self.window.show() {
            tracing::warn!(%error, "overlay show failed");
        }
    }

    /// done 相位的自动淡出观察。任何退出路径都先比对代际：
    /// 期间出现新状态（新录音/手动关闭/管道停止）时本观察作废。
    fn watch_done(&self, seq: u64, policy: &AutoFadePolicy) {
        let still_current = |seq: u64| self.generation.load(Ordering::SeqCst) == seq;

        match policy {
            AutoFadePolicy::FixedTimeout { delay } => {
                std::thread::sleep(*delay);
                if still_current(seq) {
                    self.set_state(OverlayState::hidden());
                }
            }
            AutoFadePolicy::ActivityAware {
                clock,
                poll_interval,
                fade_delay,
            } => {
                let baseline = clock.ms_since_last_input();
                // 结果出现时用户最近刚操作过（如转写期间已在打字）：
                // 视为已检测到活动，直接开始倒计时。
                let already_active = baseline <= fade_delay.as_millis() as u64;
                if !already_active {
                    // 无活动则一直保留，等新输入出现（idle 相对 baseline 变小）。
                    loop {
                        std::thread::sleep(*poll_interval);
                        if !still_current(seq) {
                            return;
                        }
                        if clock.ms_since_last_input() < baseline {
                            break;
                        }
                    }
                }
                // 倒计时不可逆：触发后不再读时钟，只看代际。
                std::thread::sleep(*fade_delay);
                if still_current(seq) {
                    self.set_state(OverlayState::hidden());
                }
            }
        }
    }
}

impl<W: OverlayWindow + Send + Sync + 'static> OverlaySink for OverlayManager<W> {
    fn set_state(&self, state: OverlayState) {
        OverlayManager::set_state(self, state);
    }
}

fn position_overlay<W: OverlayWindow>(
    window: &W,
    width: f64,
    height: f64,
    position: OverlayPosition,
) -> Result<PhysicalPosition<i32>, OverlayError> {
    let (monitor_x, monitor_y, monitor_width, monitor_height) =
        window.primary_monitor_geometry()?;
    let scale = window.scale_factor()?;
    let physical_width = (width * scale).round() as i32;
    let physical_height = (height * scale).round() as i32;
    let offset_physical = (EDGE_OFFSET * scale).round() as i32;

    let x = monitor_x + (monitor_width - physical_width) / 2;
    let y = match position {
        OverlayPosition::BottomCenter => {
            monitor_y + monitor_height - physical_height - offset_physical
        }
        OverlayPosition::TopCenter => monitor_y + offset_physical,
    };

    tracing::debug!(
        monitor_x,
        monitor_y,
        monitor_width,
        monitor_height,
        x,
        y,
        scale,
        "overlay position resolved"
    );

    Ok(PhysicalPosition::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct RecordingOverlayWindow {
        calls: Arc<Mutex<Vec<String>>>,
        monitor: Result<(i32, i32, i32, i32), String>,
        scale: Result<f64, String>,
        prepare_fails: bool,
        hide_fails: bool,
        set_size_fails: bool,
        emit_fails: bool,
        set_position_fails: bool,
    }

    impl RecordingOverlayWindow {
        fn new(monitor: (i32, i32, i32, i32), scale: f64) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                monitor: Ok(monitor),
                scale: Ok(scale),
                prepare_fails: false,
                hide_fails: false,
                set_size_fails: false,
                emit_fails: false,
                set_position_fails: false,
            }
        }

        fn with_monitor_error(scale: f64) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                monitor: Err("no monitor".into()),
                scale: Ok(scale),
                prepare_fails: false,
                hide_fails: false,
                set_size_fails: false,
                emit_fails: false,
                set_position_fails: false,
            }
        }

        fn with_prepare_error(monitor: (i32, i32, i32, i32), scale: f64) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                monitor: Ok(monitor),
                scale: Ok(scale),
                prepare_fails: true,
                hide_fails: false,
                set_size_fails: false,
                emit_fails: false,
                set_position_fails: false,
            }
        }

        fn with_scale_error() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                monitor: Ok((0, 0, 1920, 1080)),
                scale: Err("no scale".into()),
                prepare_fails: false,
                hide_fails: false,
                set_size_fails: false,
                emit_fails: false,
                set_position_fails: false,
            }
        }

        fn with_operation_error(
            monitor: (i32, i32, i32, i32),
            scale: f64,
            hide_fails: bool,
            set_size_fails: bool,
            emit_fails: bool,
            set_position_fails: bool,
        ) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                monitor: Ok(monitor),
                scale: Ok(scale),
                prepare_fails: false,
                hide_fails,
                set_size_fails,
                emit_fails,
                set_position_fails,
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn record(&self, call: impl Into<String>) {
            self.calls.lock().unwrap().push(call.into());
        }
    }

    impl OverlayWindow for RecordingOverlayWindow {
        fn emit_state(&self, state: &OverlayState) -> Result<(), OverlayError> {
            self.record(format!("emit:{}", state.phase.as_str()));
            if self.emit_fails {
                return Err(OverlayError::EmitFailed("forced".into()));
            }
            Ok(())
        }

        fn set_size(&self, size: LogicalSize<f64>) -> Result<(), OverlayError> {
            self.record(format!("size:{:.0}x{:.0}", size.width, size.height));
            if self.set_size_fails {
                return Err(OverlayError::SetSizeFailed("forced".into()));
            }
            Ok(())
        }

        fn set_position(&self, position: PhysicalPosition<i32>) -> Result<(), OverlayError> {
            self.record(format!("position:{},{}", position.x, position.y));
            if self.set_position_fails {
                return Err(OverlayError::SetPositionFailed("forced".into()));
            }
            Ok(())
        }

        fn prepare_for_show(&self) -> Result<(), OverlayError> {
            self.record("prepare_for_show");
            if self.prepare_fails {
                return Err(OverlayError::PrepareForShowFailed("forced".into()));
            }
            Ok(())
        }

        fn show(&self) -> Result<(), OverlayError> {
            self.record("show");
            Ok(())
        }

        fn hide(&self) -> Result<(), OverlayError> {
            self.record("hide");
            if self.hide_fails {
                return Err(OverlayError::HideFailed("forced".into()));
            }
            Ok(())
        }

        fn scale_factor(&self) -> Result<f64, OverlayError> {
            self.record("scale_factor");
            self.scale.clone().map_err(OverlayError::ScaleFactorFailed)
        }

        fn primary_monitor_geometry(&self) -> Result<(i32, i32, i32, i32), OverlayError> {
            self.record("primary_monitor_geometry");
            self.monitor
                .clone()
                .map_err(OverlayError::PrimaryMonitorFailed)
        }
    }

    #[test]
    fn test_visible_state_calls_window_in_order() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        manager.set_state(OverlayState::recording());

        assert_eq!(
            window.calls(),
            vec![
                "size:280x64",
                "primary_monitor_geometry",
                "scale_factor",
                "position:820,936",
                "emit:recording",
                "prepare_for_show",
                "show",
            ]
        );
    }

    #[test]
    fn test_hidden_state_emits_then_hides_after_delay() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        manager.set_state(OverlayState::hidden());

        // hide 是延迟执行的（给前端 exit 动画留时间），立即检查时不应出现。
        assert_eq!(window.calls(), vec!["emit:hidden"]);

        std::thread::sleep(HIDE_DELAY + Duration::from_millis(100));
        assert_eq!(window.calls(), vec!["emit:hidden", "hide"]);
    }

    #[test]
    fn test_pending_hide_is_cancelled_by_newer_state() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        // hidden 的延迟 hide 还没执行，用户又开始录音：
        // 旧 hide 不得把新内容关掉。
        manager.set_state(OverlayState::hidden());
        manager.set_state(OverlayState::recording());

        std::thread::sleep(HIDE_DELAY + Duration::from_millis(100));
        assert!(!window.calls().contains(&"hide".to_string()));
    }

    #[test]
    fn test_visible_state_shows_even_when_positioning_fails() {
        let window = RecordingOverlayWindow::with_monitor_error(1.0);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        manager.set_state(OverlayState::recording());

        assert_eq!(
            window.calls(),
            vec![
                "size:280x64",
                "primary_monitor_geometry",
                "emit:recording",
                "prepare_for_show",
                "show",
            ]
        );
    }

    #[test]
    fn test_visible_state_shows_and_emits_when_prepare_fails() {
        let window = RecordingOverlayWindow::with_prepare_error((0, 0, 1920, 1080), 1.0);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        manager.set_state(OverlayState::recording());

        let calls = window.calls();
        assert!(calls.contains(&"prepare_for_show".to_string()));
        assert!(calls.contains(&"show".to_string()));
        assert!(calls.contains(&"emit:recording".to_string()));
        let show_idx = calls.iter().position(|c| c == "show").unwrap();
        let emit_idx = calls.iter().position(|c| c == "emit:recording").unwrap();
        assert!(emit_idx < show_idx);
    }

    #[test]
    fn test_position_overlay_applies_scale_factor() {
        let window = RecordingOverlayWindow::new((100, 50, 3840, 2160), 2.0);
        let position =
            position_overlay(&window, 200.0, 48.0, OverlayPosition::BottomCenter).unwrap();

        assert_eq!(position, PhysicalPosition::new(1820, 1954));
    }

    #[test]
    fn test_hide_failure_warns_but_continues() {
        let window = RecordingOverlayWindow::with_operation_error(
            (0, 0, 1920, 1080),
            1.0,
            true,
            false,
            false,
            false,
        );
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        manager.set_state(OverlayState::hidden());
        std::thread::sleep(HIDE_DELAY + Duration::from_millis(100));

        assert!(window.calls().contains(&"emit:hidden".to_string()));
        assert!(window.calls().contains(&"hide".to_string()));
    }

    #[test]
    fn test_set_size_failure_warns_but_continues() {
        let window = RecordingOverlayWindow::with_operation_error(
            (0, 0, 1920, 1080),
            1.0,
            false,
            true,
            false,
            false,
        );
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        manager.set_state(OverlayState::recording());

        assert!(window.calls().contains(&"size:280x64".to_string()));
        assert!(window.calls().contains(&"show".to_string()));
    }

    #[test]
    fn test_emit_state_failure_warns_but_continues() {
        let window = RecordingOverlayWindow::with_operation_error(
            (0, 0, 1920, 1080),
            1.0,
            false,
            false,
            true,
            false,
        );
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        manager.set_state(OverlayState::recording());

        assert!(window.calls().contains(&"emit:recording".to_string()));
        assert!(window.calls().contains(&"show".to_string()));
    }

    #[test]
    fn test_set_position_failure_warns_but_continues() {
        let window = RecordingOverlayWindow::with_operation_error(
            (0, 0, 1920, 1080),
            1.0,
            false,
            false,
            false,
            true,
        );
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        manager.set_state(OverlayState::recording());

        assert!(window.calls().contains(&"position:820,936".to_string()));
        assert!(window.calls().contains(&"show".to_string()));
    }

    #[test]
    fn test_position_overlay_top_center() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        let position = position_overlay(&window, 280.0, 64.0, OverlayPosition::TopCenter).unwrap();

        // 顶部居中：x 与底部居中相同，y 为屏幕上沿加同样的偏移。
        assert_eq!(position, PhysicalPosition::new(820, 80));
    }

    #[test]
    fn test_scale_factor_failure_returns_err() {
        let window = RecordingOverlayWindow::with_scale_error();
        let result = position_overlay(&window, 280.0, 64.0, OverlayPosition::BottomCenter);

        assert!(result.is_err());
    }

    #[test]
    fn test_negative_x_coordinate_clamped() {
        // 显示器比悬浮窗宽度还窄（缩放 1.0）。
        let window = RecordingOverlayWindow::new((0, 0, 260, 1080), 1.0);
        let position =
            position_overlay(&window, 280.0, 64.0, OverlayPosition::BottomCenter).unwrap();

        // x 为负是因为 physical_width > monitor_width，整数除法得到 -10。
        assert_eq!(position.x, -10);
    }

    #[test]
    fn test_hidden_hidden_cancels_first_hide() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter);

        manager.set_state(OverlayState::hidden());
        std::thread::sleep(Duration::from_millis(50));
        manager.set_state(OverlayState::hidden());

        std::thread::sleep(HIDE_DELAY + Duration::from_millis(100));
        let calls = window.calls();
        assert_eq!(
            calls.iter().filter(|c| *c == "hide").count(),
            1,
            "expected exactly one hide after generation bump, got {:?}",
            calls
        );
    }

    #[test]
    fn test_position_overlay_non_integer_rounds() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.5);
        let position =
            position_overlay(&window, 280.0, 64.0, OverlayPosition::BottomCenter).unwrap();

        let physical_width = (280.0f64 * 1.5).round() as i32; // 420
        let physical_height = (64.0f64 * 1.5).round() as i32; // 96
        let offset = (80.0f64 * 1.5).round() as i32; // 120
        let expected_x = (1920 - physical_width) / 2; // 750
        let expected_y = 1080 - physical_height - offset; // 864
        assert_eq!(position, PhysicalPosition::new(expected_x, expected_y));
    }

    // -----------------------------------------------------------------------
    // 自动淡出（done 相位的退出策略）测试
    // -----------------------------------------------------------------------

    /// 受控活动时钟：测试中直接设置“距上次输入的毫秒数”。
    #[derive(Clone)]
    struct FakeClock {
        idle_ms: Arc<Mutex<u64>>,
    }

    impl FakeClock {
        fn new(idle_ms: u64) -> Self {
            Self {
                idle_ms: Arc::new(Mutex::new(idle_ms)),
            }
        }

        fn set(&self, idle_ms: u64) {
            *self.idle_ms.lock().unwrap() = idle_ms;
        }
    }

    impl crate::overlay::activity::UserActivityClock for FakeClock {
        fn ms_since_last_input(&self) -> u64 {
            *self.idle_ms.lock().unwrap()
        }
    }

    /// 等待 window 出现（或确认不出现）目标调用，避免固定 sleep 的脆弱性。
    fn wait_for_call(window: &RecordingOverlayWindow, call: &str, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if window.calls().iter().any(|c| c == call) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        window.calls().iter().any(|c| c == call)
    }

    #[test]
    fn test_done_auto_fades_after_fixed_timeout() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter)
            .with_auto_fade(AutoFadePolicy::FixedTimeout {
                delay: Duration::from_millis(50),
            });

        manager.set_state(OverlayState::done());

        // 淡出 = fade delay + hidden 的 HIDE_DELAY 退出动画。
        assert!(
            wait_for_call(
                &window,
                "emit:hidden",
                Duration::from_millis(50) + HIDE_DELAY + Duration::from_millis(500)
            ),
            "done 后应按固定超时自动淡出，got {:?}",
            window.calls()
        );
        assert!(wait_for_call(&window, "hide", Duration::from_millis(500)));
    }

    #[test]
    fn test_fixed_timeout_fade_cancelled_by_newer_state() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter)
            .with_auto_fade(AutoFadePolicy::FixedTimeout {
                delay: Duration::from_millis(80),
            });

        manager.set_state(OverlayState::done());
        std::thread::sleep(Duration::from_millis(20));
        // 淡出倒计时期间用户开始新一轮录音：旧定时器不得把新内容关掉。
        manager.set_state(OverlayState::recording());

        std::thread::sleep(Duration::from_millis(80) + HIDE_DELAY + Duration::from_millis(100));
        let calls = window.calls();
        assert!(
            !calls.contains(&"emit:hidden".to_string()),
            "新状态后不得自动隐藏，got {:?}",
            calls
        );
        assert!(!calls.contains(&"hide".to_string()));
    }

    #[test]
    fn test_activity_aware_keeps_done_when_user_idle() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        let clock = FakeClock::new(10_000);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter)
            .with_auto_fade(AutoFadePolicy::ActivityAware {
                clock: Arc::new(clock.clone()),
                poll_interval: Duration::from_millis(15),
                fade_delay: Duration::from_millis(40),
            });

        manager.set_state(OverlayState::done());

        // 用户一直没碰电脑（idle 持续为 10s，无新输入）：悬浮窗必须一直保留。
        std::thread::sleep(Duration::from_millis(200));
        let calls = window.calls();
        assert!(
            !calls.contains(&"emit:hidden".to_string()),
            "无输入活动时 done 悬浮窗不得自动隐藏，got {:?}",
            calls
        );

        // 清理：发出新状态让观察线程退出。
        manager.set_state(OverlayState::recording());
        std::thread::sleep(Duration::from_millis(60));
    }

    #[test]
    fn test_activity_aware_fades_on_new_input_irreversibly() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        let clock = FakeClock::new(10_000);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter)
            .with_auto_fade(AutoFadePolicy::ActivityAware {
                clock: Arc::new(clock.clone()),
                poll_interval: Duration::from_millis(15),
                fade_delay: Duration::from_millis(500),
            });

        manager.set_state(OverlayState::done());
        std::thread::sleep(Duration::from_millis(30));
        // 用户操作了电脑（idle 突然变小，说明有新输入）。
        clock.set(200);
        // 倒计时触发后用户又停手（idle 变大）：倒计时不可逆，仍应淡出。
        // 先等若干个轮询周期，确保观察线程已在新输入上触发倒计时，再拨大 idle。
        std::thread::sleep(Duration::from_millis(100));
        clock.set(60_000);

        assert!(
            wait_for_call(
                &window,
                "emit:hidden",
                Duration::from_millis(500) + HIDE_DELAY + Duration::from_millis(500)
            ),
            "检测到输入活动后应淡出（不可逆），got {:?}",
            window.calls()
        );
    }

    #[test]
    fn test_activity_aware_fades_immediately_when_user_was_just_active() {
        let window = RecordingOverlayWindow::new((0, 0, 1920, 1080), 1.0);
        // 结果出现前 30 ms 用户刚敲过键盘（转写期间已在打字）：
        // 视为“正在操作”，结果一出现就开始倒计时。
        let clock = FakeClock::new(30);
        let manager = OverlayManager::new(window.clone(), OverlayPosition::BottomCenter)
            .with_auto_fade(AutoFadePolicy::ActivityAware {
                clock: Arc::new(clock),
                poll_interval: Duration::from_millis(15),
                fade_delay: Duration::from_millis(40),
            });

        manager.set_state(OverlayState::done());

        assert!(
            wait_for_call(
                &window,
                "emit:hidden",
                Duration::from_millis(40) + HIDE_DELAY + Duration::from_millis(500)
            ),
            "结果出现时用户正在操作，应直接进入淡出倒计时，got {:?}",
            window.calls()
        );
    }
}
