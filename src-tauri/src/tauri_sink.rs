//! Tauri 管道事件接收器实现。
//!
//! 将管道事件转发为 Tauri 事件与悬浮窗状态切换：sink 只做 emit + 状态切换，
//! 不再持有 `Output` / `HistoryStore` 等业务依赖。
//!
//! 剪贴板写入与历史追加业务由 `voice_pipeline::TranscriptionDispatch`
//! trait 注入（本模块不直接调用 `process_transcription_result`）。
//! 悬浮窗物理操作由 `OverlaySink` trait 注入（本模块只描述相位意图）。
//! 框架事件发射由 `PipelineEventEmitter` trait 注入，方便测试注入 fake，
//! 无需构造真实 Wry app。

use std::time::Duration;

use std::sync::Arc;
use tauri::Emitter;

use crate::{
    config,
    error::UserFacingError,
    overlay::seam::{OverlaySink, OverlayState},
    pipeline_controller::PipelineStatus,
    voice_pipeline::{PipelineSink, TranscriptionDispatch, TranscriptionResult},
};

/// 管道事件发射 seam。
///
/// `TauriPipelineSink` 把全部 `app.emit(...)` 操作收敛到这里，
/// 生产环境由 `TauriEventEmitter` 转发给 `tauri::AppHandle`，
/// 测试环境注入 `MockEmitter` 即可断言事件内容与顺序。
pub trait PipelineEventEmitter: Send + Sync + 'static {
    fn emit_pipeline_status(&self, status: &str);
    fn emit_pipeline_error(&self, error: &UserFacingError);
    fn emit_transcription_result(&self, text: &str);
    fn emit_polish_failed(&self, error: &UserFacingError);
    fn emit_transcription_progress(&self, phase: &str, fraction: Option<f32>);
    fn emit_audio_level(&self, level: f32);
    fn emit_key_listener_backend(&self, backend: &str);
    fn emit_history_updated(&self);
    fn emit_pending_recording(&self, pending: Option<&crate::voice_pipeline::PendingRecordingInfo>);
}

/// 生产实现：把事件转发给 Tauri 前端。
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

    fn emit_pipeline_error(&self, error: &UserFacingError) {
        let _ = self.app.emit("pipeline-error", error);
    }

    fn emit_transcription_result(&self, text: &str) {
        let _ = self.app.emit("transcription-result", text);
    }

    fn emit_polish_failed(&self, error: &UserFacingError) {
        let _ = self.app.emit("polish-failed", error);
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

    fn emit_pending_recording(
        &self,
        pending: Option<&crate::voice_pipeline::PendingRecordingInfo>,
    ) {
        let _ = self.app.emit(
            crate::voice_pipeline::pending::PENDING_RECORDING_EVENT,
            pending,
        );
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
/// 与前端 `frontend/src/overlay.tsx` 的 `TRACE_SAMPLE_INTERVAL_MS`（100 ms）对齐，
/// 使 `audio-level` 事件频率成为固定 10 次/秒的跨平台契约，
/// 不再依赖平台音频后端的块/回调节奏（Linux parecord 约 31.8 ms/块，
/// Windows cpal 回调节奏不同）。
const AUDIO_LEVEL_EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// `audio-level` 定时派发状态：最新电平与 ticker 任务句柄。
///
/// `on_audio_level` 只写入 `latest`、从不直接派发，派发时机完全由 ticker
/// 任务的定时器驱动。`interval` 为派发周期（生产环境取
/// `AUDIO_LEVEL_EMIT_INTERVAL`，测试可注入更短值），`ticker` 保存当前任务
/// 句柄供状态切换时 abort。
struct AudioLevelState {
    interval: Duration,
    latest: Option<f32>,
    ticker: Option<tauri::async_runtime::JoinHandle<()>>,
}

/// `audio-level` 定时派发循环：每 `interval` 短暂持锁读取最新电平，`Some` 则派发。
///
/// 事件频率由定时器严格驱动（生产 interval 下固定 10 次/秒），与平台音频块/回调
/// 的到达节奏完全解耦，前端采样窗口每窗恰好采到一个事件，轨迹保持 10 帧/秒、
/// 10 秒窗口。Recording 期间首个事件最迟一个 interval 内到达。每 tick 拷贝
/// `latest` 后立即释放锁，锁不跨 await。
async fn run_audio_level_ticker(
    audio_level: Arc<std::sync::Mutex<AudioLevelState>>,
    emitter: Arc<dyn PipelineEventEmitter>,
    interval: Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let latest = audio_level.lock().ok().and_then(|state| state.latest);
        if let Some(level) = latest {
            emitter.emit_audio_level(level);
        }
    }
}

/// Tauri 管道事件接收器：将管道事件转发为 Tauri 事件和悬浮窗状态切换。
///
/// 只持有 `dispatch: Arc<dyn TranscriptionDispatch>` 与 overlay / emitter 抽象，
/// 业务侧由调用方在构造时一次性注入。
pub struct TauriPipelineSink {
    emitter: Arc<dyn PipelineEventEmitter>,
    pipeline_status: Arc<std::sync::RwLock<PipelineStatus>>,
    prefer_polished: bool,
    dispatch: Arc<dyn TranscriptionDispatch>,
    overlay: Arc<dyn OverlaySink>,
    audio_level: Arc<std::sync::Mutex<AudioLevelState>>,
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
            audio_level: Arc::new(std::sync::Mutex::new(AudioLevelState {
                interval: AUDIO_LEVEL_EMIT_INTERVAL,
                latest: None,
                ticker: None,
            })),
        }
    }
}

impl PipelineSink for TauriPipelineSink {
    fn on_status_change(&self, status: PipelineStatus) {
        emit_pipeline_status(&*self.emitter, &self.pipeline_status, status);

        // audio-level 派发与录音状态绑定。Recording 启动定时派发任务：先 abort
        // 旧任务防止重复派发，并清空上一段录音遗留的 latest，避免新录音先派发
        // 旧值。非 Recording（Idle/Stopped/Processing/Done）abort 任务并清空
        // latest，事件流随录音结束严格终止（Done 在下方 overlay 映射处提前
        // 返回，故须在此之前处理）。
        if status == PipelineStatus::Recording {
            if let Ok(mut state) = self.audio_level.lock() {
                if let Some(old) = state.ticker.take() {
                    old.abort();
                }
                state.latest = None;
                let ticker = tauri::async_runtime::spawn(run_audio_level_ticker(
                    Arc::clone(&self.audio_level),
                    Arc::clone(&self.emitter),
                    state.interval,
                ));
                state.ticker = Some(ticker);
            }
        } else if let Ok(mut state) = self.audio_level.lock() {
            if let Some(ticker) = state.ticker.take() {
                ticker.abort();
            }
            state.latest = None;
        }

        // 通过 OverlaySink 统一设置悬浮窗状态：一次性 emit + resize + position + show/hide。
        // recording/processing/idle/stopped 各自映射到一个 overlay 相位。
        // Done 不在此驱动（done 悬浮窗由转写完成路径异步设置）。
        let overlay_state = match status {
            PipelineStatus::Recording => OverlayState::recording(),
            PipelineStatus::Processing => OverlayState::processing(),
            PipelineStatus::Idle | PipelineStatus::Stopped => OverlayState::hidden(),
            PipelineStatus::Done => return,
        };
        self.overlay.set_state(overlay_state);
    }

    fn on_error(&self, error: &UserFacingError) {
        self.emitter.emit_pipeline_error(error);
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

                    // 润色失败先于结果文本告知前端，让悬浮窗在 done 相位能同时
                    // 展示“已回退原文”提示，文案由前端按错误码翻译。
                    if let Some(err) = output_clone.polish_error.as_ref() {
                        emitter.emit_polish_failed(err);
                    }

                    // 先送结果文本再切 done：前端收到 done 时若还没有结果，
                    // 会渲染出空 island（闪烁）。
                    emitter.emit_transcription_result(&res.text);

                    // 通过 OverlaySink 切换到 done 状态
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
        // 只记录最新电平，不在此派发：派发时机由 Recording 期间的 ticker 定时器
        // 统一驱动。
        if let Ok(mut state) = self.audio_level.lock() {
            state.latest = Some(level);
        }
    }

    fn on_key_listener_backend(&self, backend: &str) {
        self.emitter.emit_key_listener_backend(backend);
    }

    fn on_pending_recording(&self, pending: Option<&crate::voice_pipeline::PendingRecordingInfo>) {
        self.emitter.emit_pending_recording(pending);
    }
}

// 测试通过注入 `PipelineEventEmitter` fake 来验证事件内容与顺序，
// 不依赖真实 Wry app，可在 Linux 的 `cargo test --lib` 下直接运行。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::seam::OverlayPhase;
    use crate::voice_pipeline::{DispatchOutcome, TranscriptionDispatch};
    use std::collections::BTreeMap;
    use std::future::{ready, Future};
    use std::pin::Pin;
    use std::sync::Mutex;

    // -----------------------------------------------------------------------
    // 测试替身
    // -----------------------------------------------------------------------

    /// Mock `TranscriptionDispatch`：可预设返回结果。
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
    #[derive(Debug, Clone, PartialEq)]
    enum EmittedEvent {
        PipelineStatus(String),
        PipelineError(UserFacingError),
        TranscriptionResult(String),
        PolishFailed(UserFacingError),
        TranscriptionProgress {
            phase: String,
            fraction: Option<f32>,
        },
        AudioLevel(f32),
        KeyListenerBackend(String),
        HistoryUpdated,
        PendingRecording(Option<crate::voice_pipeline::PendingRecordingInfo>),
    }

    /// Mock `PipelineEventEmitter`：把每次调用按顺序记录下来。
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

        fn emit_pipeline_error(&self, error: &UserFacingError) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::PipelineError(error.clone()));
        }

        fn emit_transcription_result(&self, text: &str) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::TranscriptionResult(text.into()));
        }

        fn emit_polish_failed(&self, error: &UserFacingError) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::PolishFailed(error.clone()));
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

        fn emit_pending_recording(
            &self,
            pending: Option<&crate::voice_pipeline::PendingRecordingInfo>,
        ) {
            self.events
                .lock()
                .unwrap()
                .push(EmittedEvent::PendingRecording(pending.cloned()));
        }
    }

    // -----------------------------------------------------------------------
    // 测试夹具
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
        // Done 不在此驱动 overlay（done 悬浮窗由转写完成路径异步设置）
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
    // -----------------------------------------------------------------------

    #[test]
    fn on_error_emits_pipeline_error() {
        let fx = make_fixture(true, None);
        let with_params = UserFacingError {
            code: "transcriber.api_error".into(),
            params: Some(BTreeMap::from([
                ("status".to_string(), "429".to_string()),
                ("body".to_string(), "busy".to_string()),
            ])),
        };
        let bare = UserFacingError {
            code: "transcription.empty".into(),
            params: None,
        };
        fx.sink.on_error(&with_params);
        fx.sink.on_error(&bare);

        // 事件 payload 携带完整错误码与参数，供前端字典翻译。
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![
                EmittedEvent::PipelineError(with_params),
                EmittedEvent::PipelineError(bare),
            ]
        );
    }

    // -----------------------------------------------------------------------
    // on_transcription_result 测试
    // -----------------------------------------------------------------------

    #[test]
    fn on_transcription_result_empty_raw_text_resets_to_idle() {
        let fx = make_fixture(true, None);

        // 先把状态设为非 idle，才能观察到复位。
        *fx.status.write().unwrap() = PipelineStatus::Recording;

        fx.sink.on_transcription_result(&TranscriptionResult {
            text: String::new(),
            raw_text: String::new(),
            polish_failed: false,
            polish_error: None,
        });

        // 同步提前返回：状态必须被复位为 Idle。
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
        // 与 #[tokio::test] 的 runtime 不同，轮询等待它完成。
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
            polish_error: Some(UserFacingError {
                code: "polisher.api_error".into(),
                params: Some(BTreeMap::from([
                    ("status".to_string(), "401".to_string()),
                    ("body".to_string(), "unauthorized".to_string()),
                ])),
            }),
        });

        for _ in 0..100 {
            if fx.emitter.recorded_events().len() >= 4 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        // 失败事件先于结果文本，悬浮窗 done 相位可同时展示回退提示。
        assert_eq!(
            fx.emitter.recorded_events(),
            vec![
                EmittedEvent::HistoryUpdated,
                EmittedEvent::PipelineStatus("done".into()),
                EmittedEvent::PolishFailed(UserFacingError {
                    code: "polisher.api_error".into(),
                    params: Some(BTreeMap::from([
                        ("status".to_string(), "401".to_string()),
                        ("body".to_string(), "unauthorized".to_string()),
                    ])),
                }),
                EmittedEvent::TranscriptionResult("raw text".into()),
            ]
        );
    }

    // -----------------------------------------------------------------------
    // audio-level 定时派发测试
    // -----------------------------------------------------------------------

    /// 收集已派发的 AudioLevel 事件数值。
    fn audio_level_events(fx: &TestFixture) -> Vec<f32> {
        fx.emitter
            .recorded_events()
            .into_iter()
            .filter_map(|event| match event {
                EmittedEvent::AudioLevel(level) => Some(level),
                _ => None,
            })
            .collect()
    }

    /// 轮询等待指定电平出现在事件流中（上限约 500 ms）。
    async fn wait_for_audio_level(fx: &TestFixture, level: f32) -> bool {
        for _ in 0..100 {
            if audio_level_events(fx).contains(&level) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        false
    }

    #[test]
    fn on_audio_level_without_recording_emits_nothing() {
        let fx = make_fixture(true, None);

        // 无 ticker 运行时 on_audio_level 只记录、不派发：不应产生任何事件。
        fx.sink.on_audio_level(0.42);
        fx.sink.on_audio_level(0.9);

        assert!(fx.emitter.recorded_events().is_empty());
    }

    #[tokio::test]
    async fn recording_emits_latest_audio_level_on_ticker_cadence() {
        let fx = make_fixture(true, None);
        // 注入短 interval 缩小时钟尺度（字段私有但对本测试模块可见）。
        if let Ok(mut state) = fx.sink.audio_level.lock() {
            state.interval = Duration::from_millis(20);
        }

        fx.sink.on_status_change(PipelineStatus::Recording);
        fx.sink.on_audio_level(0.5);

        // Recording 期间首个事件最迟一个 interval 内到达。
        assert!(
            wait_for_audio_level(&fx, 0.5).await,
            "expected AudioLevel(0.5), got {:?}",
            audio_level_events(&fx)
        );

        // 周期派发持续存在：latest 不变时同一电平按 interval 节奏重复派发
        // （100 ms ≈ 5 个 20 ms tick，断下界防 flaky）。
        tokio::time::sleep(Duration::from_millis(100)).await;
        let emitted_05 = audio_level_events(&fx)
            .iter()
            .filter(|&&level| level == 0.5)
            .count();
        assert!(
            emitted_05 >= 2,
            "expected repeated 0.5 emissions, got {emitted_05}"
        );

        // latest 语义：新样本到达后，后续 tick 派发的是最新值。
        fx.sink.on_audio_level(0.9);
        assert!(
            wait_for_audio_level(&fx, 0.9).await,
            "expected a later AudioLevel(0.9), got {:?}",
            audio_level_events(&fx)
        );

        // 切 Idle 后 ticker 被 abort：事件计数不再增长（先等 100 ms 宽限吸收与
        // abort 竞态的在途派发，再观察 200 ms ≈ 10 个 tick 的静默）。
        fx.sink.on_status_change(PipelineStatus::Idle);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let baseline = audio_level_events(&fx).len();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(audio_level_events(&fx).len(), baseline);
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

    // -----------------------------------------------------------------------
    // on_pending_recording 测试
    // -----------------------------------------------------------------------

    #[test]
    fn on_pending_recording_forwards_info_and_clear() {
        let fx = make_fixture(true, None);
        let info = crate::voice_pipeline::PendingRecordingInfo {
            duration_ms: 4200,
            error: UserFacingError {
                code: "transcriber.http_error".into(),
                params: None,
            },
        };

        fx.sink.on_pending_recording(Some(&info));
        fx.sink.on_pending_recording(None);

        assert_eq!(
            fx.emitter.recorded_events(),
            vec![
                EmittedEvent::PendingRecording(Some(info)),
                EmittedEvent::PendingRecording(None),
            ]
        );
    }
}
