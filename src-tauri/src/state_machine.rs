//! 状态机模块。
//!
//! 实现了一个 5 状态的按键状态机，用于区分长按和双击两种录音模式：
//!
//! - `Idle`（空闲）→ 等待按键
//! - `PotentialPress`（潜在按下）→ 按下后等待是否达到长按阈值
//! - `Recording`（录音中）→ 长按触发，松开即停止
//! - `WaitSecondClick`（等待第二次点击）→ 短按松开后等待双击
//! - `ContinuousRecording`（连续录音）→ 双击触发，再按一次停止；按住第二次时忽略系统按键连发直至松开
//!
//! 状态机提供同步接口（`process`、`poll_timeout`、`next_deadline`），
//! 由调用方（`voice_pipeline::PipelineContext`）在 `tokio::select!` 主循环中驱动。
//!
//! State machine module.
//!
//! Implements a five-state key state machine that distinguishes two recording modes,
//! long press and double click:
//!
//! - `Idle` — waiting for a key press
//! - `PotentialPress` — pressed, waiting whether the long-press threshold is reached
//! - `Recording` — triggered by a long press; releasing stops recording
//! - `WaitSecondClick` — after a short press is released, waiting for the second click
//! - `ContinuousRecording` — triggered by double click; pressing again stops it, and
//!   system key auto-repeat during the second hold is ignored until release
//!
//! The machine exposes a synchronous interface (`process`, `poll_timeout`, `next_deadline`)
//! driven by the caller (`voice_pipeline::PipelineContext`) in its `tokio::select!` main loop.

use crate::key_listener::KeyEvent;
use std::time::{Duration, Instant};

/// 状态机发出的命令。
/// Command emitted by the state machine.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// 开始录音
    /// Start recording
    StartRecord,
    /// 停止录音
    /// Stop recording
    StopRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum State {
    Idle,
    PotentialPress,
    Recording,
    WaitSecondClick,
    ContinuousRecording,
}

/// 按键状态机，区分长按录音和双击连续录音。
///
/// 状态转换：
/// - Idle + 按下 → PotentialPress（启动长按计时器）
/// - PotentialPress + 计时器前松开 → WaitSecondClick
/// - PotentialPress + 计时器触发 → Recording（发出 StartRecord）
/// - Recording + 松开 → Idle（发出 StopRecord）
/// - WaitSecondClick + 按下 → ContinuousRecording（发出 StartRecord）
/// - WaitSecondClick + 计时器过期 → Idle（忽略）
/// - ContinuousRecording + 按下 → Idle（发出 StopRecord）
///
/// Key state machine distinguishing long-press recording from double-click continuous recording.
///
/// Transitions:
/// - Idle + press → PotentialPress (starts the long-press timer)
/// - PotentialPress + release before the timer → WaitSecondClick
/// - PotentialPress + timer fires → Recording (emits StartRecord)
/// - Recording + release → Idle (emits StopRecord)
/// - WaitSecondClick + press → ContinuousRecording (emits StartRecord)
/// - WaitSecondClick + timer expiry → Idle (ignored)
/// - ContinuousRecording + press → Idle (emits StopRecord)
pub struct Machine {
    state: State,
    long_press_threshold: Duration,
    double_click_interval: Duration,
    min_press_duration: Duration,
    press_time: Option<Instant>,
    /// 连续录音开始后直到出现一次松开前，忽略再次“按下”（避免系统按键重复在按住第二次时误触发停止）。
    /// After continuous recording starts, ignore further presses until one release occurs
    /// (so OS key auto-repeat while holding the second press cannot falsely stop it).
    continuous_hold: bool,
    /// 用“再按一次”结束连续录音后，键可能仍被按住；在收到松开前忽略按下，避免误进长按检测。
    /// After ending continuous recording with "one more press", the key may still be down;
    /// ignore presses until release to avoid slipping back into long-press detection.
    idle_suppress_press_until_release: bool,
}

impl Machine {
    /// 创建新的状态机实例。
    ///
    /// `long_press_threshold`：长按触发阈值
    /// `double_click_interval`：双击检测时间窗口
    ///
    /// Creates a new state machine instance.
    ///
    /// `long_press_threshold`: long-press trigger threshold
    /// `double_click_interval`: double-click detection time window
    pub fn new(
        long_press_threshold: Duration,
        double_click_interval: Duration,
        min_press_duration: Duration,
    ) -> Self {
        Self {
            state: State::Idle,
            long_press_threshold,
            double_click_interval,
            min_press_duration,
            press_time: None,
            continuous_hold: false,
            idle_suppress_press_until_release: false,
        }
    }

    /// 处理按键事件，返回需要发出的命令（如果有）。
    /// Handles a key event and returns the command to emit (if any).
    pub fn process(&mut self, event: KeyEvent) -> Option<Command> {
        let old_state = self.state;
        let cmd = match self.state {
            State::Idle => {
                if event.pressed && self.idle_suppress_press_until_release {
                    return None;
                }
                if !event.pressed && self.idle_suppress_press_until_release {
                    self.idle_suppress_press_until_release = false;
                    return None;
                }
                if event.pressed {
                    self.state = State::PotentialPress;
                    self.press_time = Some(Instant::now());
                }
                None
            }
            State::PotentialPress => {
                if !event.pressed {
                    // Released before long-press threshold.
                    // Reject if the press was too short — likely IME noise.
                    // 在长按阈值前松开。
                    // 若按下时长过短则拒绝——多半是输入法噪音。
                    if let Some(pt) = self.press_time {
                        if Instant::now().duration_since(pt) < self.min_press_duration {
                            // Too quick — treat as spurious IME release.
                            // Reset to Idle so a subsequent spurious release
                            // won't advance to WaitSecondClick without a real press.
                            // 过快——视为输入法的虚假松开。
                            // 回到 Idle，让后续的虚假松开
                            // 不致在没有真实按下的情况下推进到 WaitSecondClick。
                            self.state = State::Idle;
                            self.press_time = None;
                            return None;
                        }
                    }
                    self.state = State::WaitSecondClick;
                    self.press_time = Some(Instant::now());
                }
                None
            }
            State::Recording => {
                if !event.pressed {
                    self.state = State::Idle;
                    self.press_time = None;
                    Some(Command::StopRecord)
                } else {
                    None
                }
            }
            State::WaitSecondClick => {
                if event.pressed {
                    // Double click detected → continuous recording.
                    // 检测到双击 → 连续录音。
                    self.state = State::ContinuousRecording;
                    self.press_time = None;
                    self.continuous_hold = true;
                    Some(Command::StartRecord)
                } else {
                    None
                }
            }
            State::ContinuousRecording => {
                if event.pressed {
                    if self.continuous_hold {
                        return None;
                    }
                    self.state = State::Idle;
                    self.press_time = None;
                    self.continuous_hold = false;
                    self.idle_suppress_press_until_release = true;
                    Some(Command::StopRecord)
                } else {
                    self.continuous_hold = false;
                    None
                }
            }
        };
        if self.state != old_state {
            tracing::debug!(?old_state, new_state = ?self.state, "state transition");
        }
        cmd
    }

    /// 检查是否需要触发基于计时器的状态转换。
    /// Checks whether a timer-based transition should fire.
    pub fn poll_timeout(&mut self) -> Option<Command> {
        let now = Instant::now();

        match self.state {
            State::PotentialPress => {
                if let Some(pt) = self.press_time {
                    if now.duration_since(pt) >= self.long_press_threshold {
                        let old_state = self.state;
                        self.state = State::Recording;
                        self.press_time = None;
                        tracing::debug!(?old_state, new_state = ?self.state, "state transition (timeout)");
                        return Some(Command::StartRecord);
                    }
                }
                None
            }
            State::WaitSecondClick => {
                if let Some(pt) = self.press_time {
                    if now.duration_since(pt) >= self.double_click_interval {
                        let old_state = self.state;
                        self.state = State::Idle;
                        self.press_time = None;
                        tracing::debug!(?old_state, new_state = ?self.state, "state transition (timeout)");
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Returns the next deadline for a timer-based transition, if any.
    /// 返回下一次基于计时器的状态转换期限（若存在）。
    pub fn next_deadline(&self) -> Option<Instant> {
        match self.state {
            State::PotentialPress => self.press_time.map(|pt| pt + self.long_press_threshold),
            State::WaitSecondClick => self.press_time.map(|pt| pt + self.double_click_interval),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press() -> KeyEvent {
        KeyEvent { pressed: true }
    }

    fn release() -> KeyEvent {
        KeyEvent { pressed: false }
    }

    #[test]
    fn test_long_press() {
        let threshold = Duration::from_millis(50);
        let interval = Duration::from_millis(50);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        assert_eq!(sm.process(press()), None);

        std::thread::sleep(threshold + Duration::from_millis(5));
        assert_eq!(sm.poll_timeout(), Some(Command::StartRecord));

        assert_eq!(sm.process(release()), Some(Command::StopRecord));
    }

    #[test]
    fn test_double_click() {
        let threshold = Duration::from_millis(200);
        let interval = Duration::from_millis(200);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        assert_eq!(sm.process(press()), None);

        std::thread::sleep(Duration::from_millis(110));

        assert_eq!(sm.process(release()), None);

        assert_eq!(sm.process(press()), Some(Command::StartRecord));

        // Still holding (or OS key-repeat): must not stop until release.
        // 仍按住中（或系统键连发）：松开前不得停止。
        assert_eq!(sm.process(press()), None);
        assert_eq!(sm.process(release()), None);
        assert_eq!(sm.process(press()), Some(Command::StopRecord));
    }

    #[test]
    fn test_single_click_ignored() {
        let threshold = Duration::from_millis(200);
        let interval = Duration::from_millis(200);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        assert_eq!(sm.process(press()), None);

        std::thread::sleep(Duration::from_millis(110));

        assert_eq!(sm.process(release()), None);

        std::thread::sleep(interval + Duration::from_millis(5));
        assert_eq!(sm.poll_timeout(), None);
    }

    #[test]
    fn test_continuous_recording_stop() {
        let threshold = Duration::from_millis(200);
        let interval = Duration::from_millis(200);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        sm.process(press());
        std::thread::sleep(Duration::from_millis(110));
        sm.process(release());
        assert_eq!(sm.process(press()), Some(Command::StartRecord));

        assert_eq!(sm.process(release()), None);

        assert_eq!(sm.process(press()), Some(Command::StopRecord));
    }

    #[test]
    fn test_continuous_key_repeat_ignored() {
        let threshold = Duration::from_millis(200);
        let interval = Duration::from_millis(200);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        sm.process(press());
        std::thread::sleep(Duration::from_millis(110));
        sm.process(release());
        assert_eq!(sm.process(press()), Some(Command::StartRecord));
        assert_eq!(sm.process(press()), None);
        assert_eq!(sm.process(release()), None);
    }

    #[test]
    fn test_stop_continuous_suppresses_idle_press_until_release() {
        let threshold = Duration::from_millis(200);
        let interval = Duration::from_millis(200);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        sm.process(press());
        std::thread::sleep(Duration::from_millis(110));
        sm.process(release());
        assert_eq!(sm.process(press()), Some(Command::StartRecord));
        assert_eq!(sm.process(release()), None);
        assert_eq!(sm.process(press()), Some(Command::StopRecord));
        assert_eq!(sm.process(press()), None);
        assert_eq!(sm.process(release()), None);
        assert_eq!(sm.state, State::Idle);
    }

    #[test]
    fn test_recording_pressed_event_ignored() {
        let threshold = Duration::from_millis(200);
        let interval = Duration::from_millis(200);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        sm.process(press());
        std::thread::sleep(threshold + Duration::from_millis(5));
        assert_eq!(sm.poll_timeout(), Some(Command::StartRecord));
        assert_eq!(sm.state, State::Recording);

        // Another pressed event while already recording must be ignored.
        // 已在录音时的另一个按下事件必须被忽略。
        assert_eq!(sm.process(press()), None);
        assert_eq!(sm.state, State::Recording);

        assert_eq!(sm.process(release()), Some(Command::StopRecord));
        assert_eq!(sm.state, State::Idle);
    }

    #[test]
    fn test_wait_second_click_release_ignored() {
        let threshold = Duration::from_millis(200);
        let interval = Duration::from_millis(200);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        sm.process(press());
        std::thread::sleep(Duration::from_millis(110));
        assert_eq!(sm.process(release()), None);
        assert_eq!(sm.state, State::WaitSecondClick);

        // Additional release events in WaitSecondClick must be ignored.
        // WaitSecondClick 中多余的松开事件必须被忽略。
        assert_eq!(sm.process(release()), None);
        assert_eq!(sm.state, State::WaitSecondClick);
    }

    #[test]
    fn test_next_deadline_none_in_idle_recording_continuous() {
        let threshold = Duration::from_millis(200);
        let interval = Duration::from_millis(200);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        assert!(sm.next_deadline().is_none());

        sm.process(press());
        std::thread::sleep(threshold + Duration::from_millis(5));
        sm.poll_timeout();
        assert_eq!(sm.state, State::Recording);
        assert!(sm.next_deadline().is_none());

        sm.process(release());
        assert_eq!(sm.state, State::Idle);

        // Transition into ContinuousRecording and check deadline is None.
        // 转入 ContinuousRecording，并检查期限为 None。
        sm.process(press());
        std::thread::sleep(Duration::from_millis(110));
        sm.process(release());
        assert_eq!(sm.process(press()), Some(Command::StartRecord));
        assert_eq!(sm.state, State::ContinuousRecording);
        assert!(sm.next_deadline().is_none());
    }

    #[test]
    fn test_min_press_duration_boundary() {
        let threshold = Duration::from_millis(300);
        let interval = Duration::from_millis(300);
        // A min_press_duration equal to the threshold makes short presses rejected.
        // min_press_duration 等于阈值时，短按会被拒绝。
        let min_press = threshold;
        let mut sm = Machine::new(threshold, interval, min_press);

        assert_eq!(sm.process(press()), None);

        // Release immediately: duration is strictly less than min_press_duration.
        // 立即松开：时长严格小于 min_press_duration。
        assert_eq!(sm.process(release()), None);
        assert_eq!(sm.state, State::Idle);

        // Verify exactly min_press_duration is accepted: sleep at least that long.
        // 验证恰好达到 min_press_duration 即被接受：至少睡够这么久。
        assert_eq!(sm.process(press()), None);
        std::thread::sleep(min_press + Duration::from_millis(5));
        assert_eq!(sm.process(release()), None);
        assert_eq!(sm.state, State::WaitSecondClick);
    }

    #[test]
    fn test_idle_suppress_press_until_release_reset() {
        let threshold = Duration::from_millis(200);
        let interval = Duration::from_millis(200);
        let min_press = Duration::from_millis(1);
        let mut sm = Machine::new(threshold, interval, min_press);

        // Enter continuous recording and stop it, which sets the suppress flag.
        // 进入连续录音并停止它，这一步会设置抑制标志。
        sm.process(press());
        std::thread::sleep(Duration::from_millis(110));
        sm.process(release());
        assert_eq!(sm.process(press()), Some(Command::StartRecord));
        assert_eq!(sm.process(release()), None);
        assert_eq!(sm.process(press()), Some(Command::StopRecord));
        assert_eq!(sm.state, State::Idle);
        assert!(sm.idle_suppress_press_until_release);

        // Presses are ignored while the flag is set and the key is still down.
        // 标志置位且键仍被按住时，按下被忽略。
        assert_eq!(sm.process(press()), None);
        assert_eq!(sm.state, State::Idle);

        // The release resets the flag; subsequent press is processed normally.
        // 松开会重置标志；其后的按下照常处理。
        assert_eq!(sm.process(release()), None);
        assert!(!sm.idle_suppress_press_until_release);
        assert_eq!(sm.process(press()), None);
        assert_eq!(sm.state, State::PotentialPress);
    }
}
