//! Tauri 管道事件接收器实现。
//!
//! 将管道事件转发为 Tauri 事件与浮窗状态切换：sink 只做 emit + 状态切换，
//! 不再持有 `Output` / `HistoryStore` 等业务依赖。
//!
//! 剪贴板写入与历史追加业务由 `voice_pipeline::TranscriptionDispatch`
//! trait 注入（本模块不直接调用 `process_transcription_result`）；
//! 浮窗物理操作由 `OverlaySink` trait 注入（本模块只描述阶段意图）；
//! 框架事件发射由 `PipelineEventEmitter` trait 注入，方便测试注入 fake，
//! 无需构造真实 Wry app。
//!
//! Tauri pipeline event sink implementation.
//!
//! Forwards pipeline events into Tauri events and overlay state switches: the sink only emits and
//! switches states, holding no business dependencies like `Output` / `HistoryStore`.
//!
//! Clipboard-write and history-append business is injected through the
//! `voice_pipeline::TranscriptionDispatch` trait (this module never calls
//! `process_transcription_result` directly); physical overlay operations are injected through the
//! `OverlaySink` trait (this module only describes phase intent); framework event emission is
//! injected through the `PipelineEventEmitter` trait, so tests can plug in fakes without building
//! a real Wry app.

use std::time::{Duration, Instant};

use std::sync::Arc;
use tauri::Emitter;

use crate::{
    config,
    overlay::seam::{OverlaySink, OverlayState},
    pipeline_controller::PipelineStatus,
    voice_pipeline::{PipelineSink, TranscriptionDispatch, TranscriptionResult},
};

/// 管道事件发射 seam。
///
/// `TauriPipelineSink` 把全部 `app.emit(...)` 操作收敛到这里，
/// 生产环境由 `TauriEventEmitter` 转发给 `tauri::AppHandle`，
/// 测试环境注入 `MockEmitter` 即可断言事件内容与顺序。
///
/// Pipeline event emission seam.
///
/// `TauriPipelineSink` funnels all `app.emit(...)` calls through here: production forwards them to
/// `tauri::AppHandle` via `TauriEventEmitter`, while tests inject a `MockEmitter` and assert on
/// event content and ordering.
pub trait PipelineEventEmitter: Send + Sync + 'static {
    fn emit_pipeline_status(&self, status: &str);
    fn emit_pipeline_error(&self, message: &str);
    fn emit_transcription_result(&self, text: &str);
    fn emit_polish_failed(&self, message: &str);
    fn emit_transcription_progress(&self, phase: &str, fraction: Option<f32>);
    fn emit_audio_level(&self, level: f32);
    fn emit_key_listener_backend(&self, backend: &str);
    fn emit_history_updated(&self);
}

/// 生产实现：把事件转发给 Tauri 前端。
/// Production implementation: forwards events to the Tauri frontend.
pub struct TauriEventEmitter {
    app: tauri::AppHandle,
}

impl TauriEventEmitter {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl PipelineEventEmitter for TauriEventEmitter {
    fn emit_pipeline_status(&self, status: &str) {
        let _ = self.app.emit("pipeline-status", status);
    }

    fn emit_pipeline_error(&self, message: &str) {
        let _ = self.app.emit("pipeline-error", message);
    }

    fn emit_transcription_result(&self, text: &str) {
        let _ = self.app.emit("transcription-result", text);
    }

    fn emit_polish_failed(&self, message: &str) {
        let _ = self.app.emit("polish-failed", message);
    }

    fn emit_transcription_progress(&self, phase: &str, fraction: Option<f32>) {
        let _ = self.app.emit(
            "transcription-progress",
            serde_json::json!({ "phase": phase, "fraction": fraction }),
        );
    }

    fn emit_audio_level(&self, level: f32) {
        let _ = self.app.emit("audio-level", level);
    }

    fn emit_key_listener_backend(&self, backend: &str) {
        let _ = self.app.emit("key-listener-backend", backend);
    }

    fn emit_history_updated(&self) {
        let _ = self.app.emit("history-updated", ());
    }
}

fn emit_pipeline_status(
    emitter: &dyn PipelineEventEmitter,
    status: &Arc<std::sync::RwLock<PipelineStatus>>,
    value: PipelineStatus,
) {
    emitter.emit_pipeline_status(value.as_str());
    if let Ok(mut s) = status.write() {
        *s = value;
    }
}

/// `audio-level` 事件的固定派发间隔。
///
/// 与前端 `frontend/src/overlay.tsx` 的 `TRACE_SAMPLE_INTERVAL_MS`（100ms）对齐，
/// 使 `audio-level` 事件频率成为固定 10 次/秒的跨平台契约，
/// 不再依赖平台音频后端的块/回调节奏（Linux parecord 约 31.8ms/块，
/// Windows cpal 回调节奏不同）。
///
/// Fixed emission interval for `audio-level` events.
///
/// Aligned with `TRACE_SAMPLE_INTERVAL_MS` (100ms) in `frontend/src/overlay.tsx`,
/// making the `audio-level` event rate a fixed 10-per-second cross-platform contract
/// that no longer depends on platform audio block/callback cadence (Linux parecord
/// ~31.8ms per block; Windows cpal callbacks run at a different cadence).
const AUDIO_LEVEL_EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// `audio-level` 事件的 leading-edge 节流器：首个样本立即放行，
/// 之后每过一个 `interval` 才放行一次，间隔内的样本直接丢弃。
///
/// Leading-edge throttle for `audio-level` events: the first sample passes through
/// immediately, then one sample per `interval` is admitted; samples inside the
/// interval are dropped.
struct AudioLevelThrottle {
    interval: Duration,
    last_emit: Option<Instant>,
}

impl AudioLevelThrottle {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_emit: None,
        }
    }

    /// 尚未放过样本（或已重置）时立即放行；否则距上次放行不足 `interval` 丢弃，
    /// 达到/超过 `interval` 放行并重记时间窗。返回 `Some(level)` 表示应派发。
    ///
    /// Admits immediately when no sample has passed yet (or the throttle was reset);
    /// otherwise drops while less than `interval` has elapsed since the last admission,
    /// and admits once the elapsed time reaches `interval`, restarting the window.
    /// `Some(level)` means it should be emitted.
    fn accept(&mut self, level: f32, now: Instant) -> Option<f32> {
        if self
            .last_emit
            .is_none_or(|last| now.duration_since(last) >= self.interval)
        {
            self.last_emit = Some(now);
            Some(level)
        } else {
            None
        }
    }
}

/// Tauri 管道事件接收器 — 将管道事件转发为 Tauri 事件和浮窗状态切换。
///
/// 只持有 `dispatch: Arc<dyn TranscriptionDispatch>` 与 overlay / emitter 抽象，
/// 业务侧由调用方在构造时一次性注入。
///
/// Tauri pipeline event sink — turns pipeline events into Tauri events plus overlay state switches.
///
/// Holds only `dispatch: Arc<dyn TranscriptionDispatch>` plus the overlay/emitter abstractions;
/// the business side is injected once by the caller at construction.
pub struct TauriPipelineSink {
    emitter: Arc<dyn PipelineEventEmitter>,
    pipeline_status: Arc<std::sync::RwLock<PipelineStatus>>,
    prefer_polished: bool,
    dispatch: Arc<dyn TranscriptionDispatch>,
    overlay: Arc<dyn OverlaySink>,
    audio_level_throttle: std::sync::Mutex<AudioLevelThrottle>,
}

impl TauriPipelineSink {
    pub fn new(
        emitter: Arc<dyn PipelineEventEmitter>,
        pipeline_status: Arc<std::sync::RwLock<PipelineStatus>>,
        cfg: Arc<config::Config>,
        dispatch: Arc<dyn TranscriptionDispatch>,
        overlay: Arc<dyn OverlaySink>,
    ) -> Self {
        Self {
            emitter,
            pipeline_status,
            prefer_polished: cfg.output.prefer_polished,
            dispatch,
            overlay,
            audio_level_throttle: std::sync::Mutex::new(AudioLevelThrottle::new(
                AUDIO_LEVEL_EMIT_INTERVAL,
            )),
        }
    }
}

impl PipelineSink for TauriPipelineSink {
    fn on_status_change(&self, status: PipelineStatus) {
        emit_pipeline_status(&*self.emitter, &self.pipeline_status, status);

        // 录音开始时重置节流器：上次录音遗留的 last_emit 可能吞掉本次录音的
        // 首个电平样本，重置后首个样本立即派发。
        // Reset the throttle when recording starts: a stale last_emit from the previous
        // recording could swallow this recording's first level sample; after the reset
        // the first sample is admitted immediately.
        if status == PipelineStatus::Recording {
            if let Ok(mut throttle) = self.audio_level_throttle.lock() {
                *throttle = AudioLevelThrottle::new(AUDIO_LEVEL_EMIT_INTERVAL);
            }
        }

        // 通过 OverlaySink 统一设置悬浮窗状态 —— 一次性 emit + resize + position + show/hide。
        // recording/processing/idle/stopped 各自映射到一个 overlay 阶段；
        // Done 不在此驱动（done 浮窗由转写完成路径异步设置）。
        // Sets the overlay state uniformly through OverlaySink—one emit + resize + position + show/hide.
        // recording/processing/idle/stopped each map onto one overlay phase;
        // Done is not driven here (the done overlay is set asynchronously by the transcription-
        // completion path).
        let overlay_state = match status {
            PipelineStatus::Recording => OverlayState::recording(),
            PipelineStatus::Processing => OverlayState::processing(),
            PipelineStatus::Idle | PipelineStatus::Stopped => OverlayState::hidden(),
            PipelineStatus::Done => return,
        };
        self.overlay.set_state(overlay_state);
    }

    fn on_error(&self, message: &str) {
        self.emitter.emit_pipeline_error(message);
    }

    fn on_transcription_result(&self, output: &TranscriptionResult) {
        if output.raw_text.is_empty() {
            emit_pipeline_status(&*self.emitter, &self.pipeline_status, PipelineStatus::Idle);
            return;
        }

        let emitter = Arc::clone(&self.emitter);
        let status = self.pipeline_status.clone();
        let output_clone = output.clone();
        let prefer_polished = self.prefer_polished;
        let dispatch = Arc::clone(&self.dispatch);
        let overlay = self.overlay.clone();

        tauri::async_runtime::spawn(async move {
            let result = dispatch.dispatch(&output_clone, prefer_polished).await;

            match result {
                Some(res) => {
                    if res.history_appended {
                        emitter.emit_history_updated();
                    }

                    emit_pipeline_status(&*emitter, &status, PipelineStatus::Done);

                    // 润色失败先于结果文本告知前端，让悬浮窗在 done 阶段能同时
                    // 展示「已回退原文」提示。
                    // The polish failure reaches the frontend before the result text, letting the done overlay
                    // also show the "fell back to raw text" hint.
                    if output_clone.polish_failed {
                        emitter.emit_polish_failed(
                            output_clone
                                .polish_error
                                .as_deref()
                                .unwrap_or("润色失败，已使用原文"),
                        );
                    }

                    // 先送结果文本再切 done：前端收到 done 时若还没有结果，
                    // 会渲染出空 island（闪烁）。
                    // Result text goes before switching to done: if done arrived without a result, the frontend
                    // would render an empty island (flicker).
                    emitter.emit_transcription_result(&res.text);

                    // 通过 OverlaySink 切换到 done 状态
                    // Switch to the done state through OverlaySink
                    overlay.set_state(OverlayState::done());
                }
                None => {
                    emit_pipeline_status(&*emitter, &status, PipelineStatus::Idle);
                }
            }
        });
    }

    fn on_progress(&self, phase: &str, fraction: Option<f32>) {
        self.emitter.emit_transcription_progress(phase, fraction);
    }

    fn on_audio_level(&self, level: f32) {
        if let Ok(mut throttle) = self.audio_level_throttle.lock() {
            if let Some(level) = throttle.accept(level, Instant::now()) {
                self.emitter.emit_audio_level(level);
            }
        }
    }

    fn on_key_listener_backend(&self, backend: &str) {
        self.emitter.emit_key_listener_backend(backend);
    }
}

// 测试通过注入 `PipelineEventEmitter` fake 来验证事件内容与顺序，
// 不依赖真实 Wry app，可在 Linux 的 `cargo test --lib` 下直接运行。
// Tests inject a `PipelineEventEmitter` fake to verify event content and ordering. No real Wry
// app needed; runs directly under Linux `cargo test --lib`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::seam::OverlayPhase;
    use crate::voice_pipeline::{DispatchOutcome, TranscriptionDispatch};
    use std::future::{ready, Future};
    use std::pin::Pin;
    use std::sync::Mutex;

    // -----------------------------------------------------------------------
    // Test doubles
    // Test doubles（测试替身）
    // -----------------------------------------------------------------------

    /// Mock `TranscriptionDispatch`：可预设返回结果。
    /// Mock `TranscriptionDispatch` with presettable outcomes.
    struct MockDispatch {
        outcome: Option<DispatchOutcome>,
    }

    impl TranscriptionDispatch for MockDispatch {
        fn dispatch<'a>(
            &'a self,
            _output: &'a TranscriptionResult,
            _prefer_polished: bool,
        ) -> Pin<Box<dyn Future<Output = Option<DispatchOutcome>> + Send + 'a>> {
            Box::pin(ready(self.outcome.clone()))
        }
    }

    /// Mock `OverlaySink`，记录每一次 `set_state` 调用。
    /// Mock `OverlaySink` that records every `set_state` call.
    struct MockOverlay {
        states: Mutex<Vec<OverlayState>>,
    }

    impl MockOverlay {
        fn new() -> Self {
            Self {
                states: Mutex::new(Vec::new()),
            }
        }

        fn recorded_states(&self) -> Vec<OverlayState> {
            self.states.lock().unwrap().clone()
        }
    }

    impl OverlaySink for MockOverlay {
        fn set_state(&self, state: OverlayState) {
            self.states.lock().unwrap().push(state);
        }
    }

    /// 一次记录的事件调用。
    /// One recorded event invocation.
    #[derive(Debug, Clone, PartialEq)]
    enum EmittedEvent {
        PipelineStatus(String),
        PipelineError(String),
        TranscriptionResult(String),
        PolishFailed(String),
        TranscriptionProgress {
            phase: String,
            fraction: Option<f32>,
        },
        AudioLevel(f32),
        KeyListenerBackend(String),
        HistoryUpdated,
    }

    /// Mock `PipelineEventEmitter`：把每次调用按顺序记录下来。
    /// Mock `PipelineEventEmitter`, recording every call in order.
    struct MockEmitter {
        events: Mutex<Vec<EmittedEvent>>,
    }

    impl MockEmitter {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        fn recorded_events(&self) -> Vec<EmittedEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl PipelineEventEmitter for MockEmitter {
        fn emit_pipeline_status(&self, status: &str) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::PipelineStatus(status.into()));
        }

        fn emit_pipeline_error(&self, message: &str) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::PipelineError(message.into()));
        }

        fn emit_transcription_result(&self, text: &str) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::TranscriptionResult(text.into()));
        }

        fn emit_polish_failed(&self, message: &str) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::PolishFailed(message.into()));
        }

        fn emit_transcription_progress(&self, phase: &str, fraction: Option<f32>) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::TranscriptionProgress {
                    phase: phase.into(),
                    fraction,
                });
        }

        fn emit_audio_level(&self, level: f32) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::AudioLevel(level));
        }

        fn emit_key_listener_backend(&self, backend: &str) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::KeyListenerBackend(backend.into()));
        }

        fn emit_history_updated(&self) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::HistoryUpdated);
        }
    }

    // -----------------------------------------------------------------------
    // Fixture
    // -----------------------------------------------------------------------

    struct TestFixture {
        sink: TauriPipelineSink,
        status: Arc<std::sync::RwLock<PipelineStatus>>,
        overlay: Arc<MockOverlay>,
        emitter: Arc<MockEmitter>,
    }

    fn make_fixture(
        prefer_polished: bool,
        dispatch_outcome: Option<DispatchOutcome>,
    ) -> TestFixture {
        let status = Arc::new(std::sync::RwLock::new(PipelineStatus::Idle));
        let overlay = Arc::new(MockOverlay::new());
        let emitter = Arc::new(MockEmitter::new());

        let mut cfg = config::Config::default();
        cfg.output.prefer_polished = prefer_polished;

        let dispatch: Arc<dyn TranscriptionDispatch> = Arc::new(MockDispatch {
            outcome: dispatch_outcome,
        });
        let sink = TauriPipelineSink::new(
            emitter.clone(),
            status.clone(),
            Arc::new(cfg),
            dispatch,
            overlay.clone(),
        );

        TestFixture {
            sink,
            status,
            overlay,
            emitter,
        }
    }

    // -----------------------------------------------------------------------
    // on_status_change 测试
    // on_status_change tests
    // -----------------------------------------------------------------------

    #[test]
    fn on_status_change_recording_maps_status_and_overlay() {
        let fx = make_fixture(true, None);
        fx.sink.on_status_change(PipelineStatus::Recording);

        assert_eq!(*fx.status.read().unwrap(), PipelineStatus::Recording);
        let states = fx.overlay.recorded_states();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].phase, OverlayPhase::Recording);
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![EmittedEvent::PipelineStatus("recording".into())]
        );
    }

    #[test]
    fn on_status_change_processing_maps_status_and_overlay() {
        let fx = make_fixture(true, None);
        fx.sink.on_status_change(PipelineStatus::Processing);

        assert_eq!(*fx.status.read().unwrap(), PipelineStatus::Processing);
        let states = fx.overlay.recorded_states();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].phase, OverlayPhase::Processing);
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![EmittedEvent::PipelineStatus("processing".into())]
        );
    }

    #[test]
    fn on_status_change_done_maps_status_without_overlay_call() {
        let fx = make_fixture(true, None);
        fx.sink.on_status_change(PipelineStatus::Done);

        assert_eq!(*fx.status.read().unwrap(), PipelineStatus::Done);
        // Done 不在此驱动 overlay（done 浮窗由转写完成路径异步设置）
        // Done does not drive the overlay here (done overlay set async by the completion path)
        assert!(fx.overlay.recorded_states().is_empty());
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![EmittedEvent::PipelineStatus("done".into())]
        );
    }

    #[test]
    fn on_status_change_idle_hides_overlay() {
        let fx = make_fixture(true, None);
        fx.sink.on_status_change(PipelineStatus::Idle);

        assert_eq!(*fx.status.read().unwrap(), PipelineStatus::Idle);
        let states = fx.overlay.recorded_states();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].phase, OverlayPhase::Hidden);
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![EmittedEvent::PipelineStatus("idle".into())]
        );
    }

    #[test]
    fn on_status_change_stopped_hides_overlay() {
        let fx = make_fixture(true, None);
        fx.sink.on_status_change(PipelineStatus::Stopped);

        assert_eq!(*fx.status.read().unwrap(), PipelineStatus::Stopped);
        let states = fx.overlay.recorded_states();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].phase, OverlayPhase::Hidden);
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![EmittedEvent::PipelineStatus("stopped".into())]
        );
    }

    // -----------------------------------------------------------------------
    // on_error 测试
    // on_error tests
    // -----------------------------------------------------------------------

    #[test]
    fn on_error_emits_pipeline_error() {
        let fx = make_fixture(true, None);
        fx.sink.on_error("something went wrong");
        fx.sink.on_error("");

        assert_eq!(
            fx.emitter.recorded_events(),
            vec![
                EmittedEvent::PipelineError("something went wrong".into()),
                EmittedEvent::PipelineError("".into()),
            ]
        );
    }

    // -----------------------------------------------------------------------
    // on_transcription_result 测试
    // on_transcription_result tests
    // -----------------------------------------------------------------------

    #[test]
    fn on_transcription_result_empty_raw_text_resets_to_idle() {
        let fx = make_fixture(true, None);

        // 先把状态设为非 idle，才能观察到复位。
        // Set status to something non-idle first so we can observe the reset.
        *fx.status.write().unwrap() = PipelineStatus::Recording;

        fx.sink.on_transcription_result(&TranscriptionResult {
            text: String::new(),
            raw_text: String::new(),
            polish_failed: false,
            polish_error: None,
        });

        // 同步提前返回：状态必须被复位为 Idle。
        // Synchronous early-return: status must be reset to Idle.
        assert_eq!(*fx.status.read().unwrap(), PipelineStatus::Idle);
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![EmittedEvent::PipelineStatus("idle".into())]
        );
    }

    #[tokio::test]
    async fn on_transcription_result_non_empty_dispatches_async_and_emits_in_order() {
        let fx = make_fixture(
            false,
            Some(DispatchOutcome {
                text: "polished text".into(),
                history_appended: true,
            }),
        );

        fx.sink.on_transcription_result(&TranscriptionResult {
            text: "polished".into(),
            raw_text: "raw text".into(),
            polish_failed: false,
            polish_error: None,
        });

        // spawned 任务跑在 tauri::async_runtime 的全局 runtime 上，
        // 与 #[tokio::test] 的 runtime 不同；轮询等待它完成。
        // The spawned task runs on tauri::async_runtime's global runtime, distinct from #[tokio::test]'s;
        // poll until it completes.
        for _ in 0..100 {
            if fx.emitter.recorded_events().len() >= 3 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert_eq!(*fx.status.read().unwrap(), PipelineStatus::Done);

        let overlay_states = fx.overlay.recorded_states();
        assert_eq!(overlay_states.len(), 1);
        assert_eq!(overlay_states[0].phase, OverlayPhase::Done);

        // 真实代码顺序：history-updated → pipeline-status(Done) → transcription-result(text)
        // 先送文本再切 done，前端收到 done 时已有结果，避免空 island 闪烁。
        // Real code order: history-updated → pipeline-status(Done) → transcription-result(text).
        // Text precedes the done switch, so results exist when done lands—no empty-island flicker.
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![
                EmittedEvent::HistoryUpdated,
                EmittedEvent::PipelineStatus("done".into()),
                EmittedEvent::TranscriptionResult("polished text".into()),
            ]
        );
    }

    #[tokio::test]
    async fn on_transcription_result_polish_failed_emits_event_before_text() {
        let fx = make_fixture(
            true,
            Some(DispatchOutcome {
                text: "raw text".into(),
                history_appended: true,
            }),
        );

        fx.sink.on_transcription_result(&TranscriptionResult {
            text: "raw text".into(),
            raw_text: "raw text".into(),
            polish_failed: true,
            polish_error: Some("LLM API 错误（HTTP 401）".into()),
        });

        for _ in 0..100 {
            if fx.emitter.recorded_events().len() >= 4 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        // 失败事件先于结果文本，悬浮窗 done 阶段可同时展示回退提示。
        // Failure event precedes result text; the done overlay can show the fallback hint alongside.
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![
                EmittedEvent::HistoryUpdated,
                EmittedEvent::PipelineStatus("done".into()),
                EmittedEvent::PolishFailed("LLM API 错误（HTTP 401）".into()),
                EmittedEvent::TranscriptionResult("raw text".into()),
            ]
        );
    }

    // -----------------------------------------------------------------------
    // AudioLevelThrottle / on_audio_level 测试
    // AudioLevelThrottle / on_audio_level tests
    // -----------------------------------------------------------------------

    #[test]
    fn audio_level_throttle_admits_first_sample_immediately() {
        let mut throttle = AudioLevelThrottle::new(Duration::from_millis(100));
        let now = Instant::now();

        assert_eq!(throttle.accept(0.42, now), Some(0.42));
    }

    #[test]
    fn audio_level_throttle_drops_samples_inside_interval() {
        let mut throttle = AudioLevelThrottle::new(Duration::from_millis(100));
        let t0 = Instant::now();

        assert_eq!(throttle.accept(0.42, t0), Some(0.42));
        assert_eq!(throttle.accept(0.5, t0 + Duration::from_millis(50)), None);
        assert_eq!(throttle.accept(0.7, t0 + Duration::from_millis(99)), None);
    }

    #[test]
    fn audio_level_throttle_admits_at_or_after_interval() {
        let mut throttle = AudioLevelThrottle::new(Duration::from_millis(100));
        let t0 = Instant::now();

        assert_eq!(throttle.accept(0.42, t0), Some(0.42));
        assert_eq!(
            throttle.accept(0.5, t0 + Duration::from_millis(100)),
            Some(0.5)
        );
        assert_eq!(
            throttle.accept(0.7, t0 + Duration::from_millis(250)),
            Some(0.7)
        );
    }

    #[test]
    fn audio_level_throttle_restarts_window_after_admission() {
        let mut throttle = AudioLevelThrottle::new(Duration::from_millis(100));
        let t0 = Instant::now();

        assert_eq!(throttle.accept(0.42, t0), Some(0.42));
        // 150ms 处放行后，时间窗从 150ms 重新计算：250ms（仅再过 100ms）放行，220ms 丢弃。
        // After the admission at 150ms the window restarts: 250ms (another 100ms) is admitted;
        // 220ms is dropped.
        let t150 = t0 + Duration::from_millis(150);
        assert_eq!(throttle.accept(0.5, t150), Some(0.5));
        assert_eq!(throttle.accept(0.6, t150 + Duration::from_millis(70)), None);
        assert_eq!(
            throttle.accept(0.7, t150 + Duration::from_millis(100)),
            Some(0.7)
        );
    }

    #[test]
    fn on_audio_level_throttles_rapid_samples_to_first_only() {
        let fx = make_fixture(true, None);
        fx.sink.on_audio_level(0.42);
        // 真实时钟下两次调用间隔必 <100ms，第二个样本应被节流丢弃。
        // Under the real clock the two calls are <100ms apart; the second sample must be dropped.
        fx.sink.on_audio_level(0.0);

        assert_eq!(
            fx.emitter.recorded_events(),
            vec![EmittedEvent::AudioLevel(0.42)]
        );
    }

    #[test]
    fn on_status_change_recording_resets_throttle_for_first_sample() {
        let fx = make_fixture(true, None);
        fx.sink.on_audio_level(0.42);

        fx.sink.on_status_change(PipelineStatus::Recording);
        fx.sink.on_audio_level(0.7);

        // 重置后新录音的首个电平样本立即派发（status 事件之外只有这一个 AudioLevel）。
        // After the reset the new recording's first level sample is admitted immediately
        // (besides the status event there is exactly one AudioLevel).
        let audio_levels: Vec<_> = fx
            .emitter
            .recorded_events()
            .into_iter()
            .filter(|e| matches!(e, EmittedEvent::AudioLevel(_)))
            .collect();
        assert_eq!(
            audio_levels,
            vec![
                EmittedEvent::AudioLevel(0.42),
                EmittedEvent::AudioLevel(0.7)
            ]
        );
    }

    #[test]
    fn on_progress_emits_progress() {
        let fx = make_fixture(true, None);
        fx.sink.on_progress("transcribe", Some(0.5));
        fx.sink.on_progress("polish", None);
        fx.sink.on_progress("done", Some(1.0));

        assert_eq!(
            fx.emitter.recorded_events(),
            vec![
                EmittedEvent::TranscriptionProgress {
                    phase: "transcribe".into(),
                    fraction: Some(0.5),
                },
                EmittedEvent::TranscriptionProgress {
                    phase: "polish".into(),
                    fraction: None,
                },
                EmittedEvent::TranscriptionProgress {
                    phase: "done".into(),
                    fraction: Some(1.0),
                },
            ]
        );
    }

    // -----------------------------------------------------------------------
    // on_key_listener_backend 测试
    // on_key_listener_backend tests
    // -----------------------------------------------------------------------

    #[test]
    fn on_key_listener_backend_emits_backend() {
        let fx = make_fixture(true, None);
        fx.sink.on_key_listener_backend("xinput");
        fx.sink.on_key_listener_backend("evtest");
        fx.sink.on_key_listener_backend("");

        assert_eq!(
            fx.emitter.recorded_events(),
            vec![
                EmittedEvent::KeyListenerBackend("xinput".into()),
                EmittedEvent::KeyListenerBackend("evtest".into()),
                EmittedEvent::KeyListenerBackend("".into()),
            ]
        );
    }
}
