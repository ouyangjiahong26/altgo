//! 测试替身（test doubles）：给 voice_pipeline 子模块共享。
//!
//! 这里只放 `#[cfg(test)]` 用的替身结构体 / 实现，避免每个子模块重复
//! 同一套 fake/mock。

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::error::{KeyListenerError, OutputError, UserFacingError};
use crate::key_listener::{KeyEvent, KeyListener};
use crate::output::Output;
use crate::pipeline_controller::PipelineStatus;
use crate::recorder::{AudioLevelCallback, Recorder};
use crate::transcriber::Transcriber;

use super::sink::{PipelineSink, TranscriptionResult};

type ProgressEvent = (String, Option<f32>);
type ProgressEvents = Arc<Mutex<Vec<ProgressEvent>>>;

// ---------------------------------------------------------------------------
// KeyListener fake（假按键监听）
// ---------------------------------------------------------------------------

/// [`FakeListener::new`] 返回的句柄，测试经它把按键事件注入 `start()` 创建的通道。
pub(super) struct FakeListenerHandle {
    tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<KeyEvent>>>>,
}

impl FakeListenerHandle {
    pub(super) fn send(&self, ev: KeyEvent) {
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(ev);
        }
    }
}

pub(super) struct FakeListener {
    pub(super) backend: &'static str,
    tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<KeyEvent>>>>,
}

impl FakeListener {
    pub(super) fn new(backend: &'static str) -> (Self, FakeListenerHandle) {
        let tx = Arc::new(Mutex::new(None));
        (
            Self {
                backend,
                tx: Arc::clone(&tx),
            },
            FakeListenerHandle { tx },
        )
    }
}

impl KeyListener for FakeListener {
    fn start(
        &mut self,
    ) -> Result<
        (
            tokio::sync::mpsc::UnboundedReceiver<crate::key_listener::KeyEvent>,
            &'static str,
        ),
        KeyListenerError,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        *self.tx.lock().unwrap() = Some(tx);
        Ok((rx, self.backend))
    }
}

/// Drop 即停止发送并关闭事件通道的监听器替身。
///
/// 与 `FakeListener` 的区别：`FakeListener` 的事件通道经 `Arc` 与句柄共享，
/// 监听器本体被丢弃后通道仍然打开；真实监听器（evtest 读取线程、Windows
/// 钩子）的发送端随监听器停止而消失。本替身在后台线程周期发送按下/松开，
/// 监听器被 Drop 后线程退出、通道关闭，用来回归验证主循环持有监听器的
/// 生命周期约束。
pub(super) struct DropClosingListener {
    alive: Arc<std::sync::atomic::AtomicBool>,
}

impl DropClosingListener {
    pub(super) fn new() -> Self {
        Self {
            alive: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }
}

impl KeyListener for DropClosingListener {
    fn start(
        &mut self,
    ) -> Result<
        (
            tokio::sync::mpsc::UnboundedReceiver<crate::key_listener::KeyEvent>,
            &'static str,
        ),
        KeyListenerError,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let alive = Arc::clone(&self.alive);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(10));
                if !alive.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let _ = tx.send(KeyEvent { pressed: true });
                // 按住时长须超过测试上下文的 60 ms 长按阈值以触发录音。
                std::thread::sleep(std::time::Duration::from_millis(80));
                if !alive.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let _ = tx.send(KeyEvent { pressed: false });
            }
            // 线程退出时丢弃 tx，事件通道随之关闭，与真实监听器一致。
        });
        Ok((rx, "drop-closing"))
    }
}

impl Drop for DropClosingListener {
    fn drop(&mut self) {
        self.alive.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Recorder fake（假录音器）
// ---------------------------------------------------------------------------

pub(super) struct FakeRecorder {
    pub(super) recording: std::sync::atomic::AtomicBool,
    pub(super) audio: Vec<u8>,
    pub(super) start_count: std::sync::atomic::AtomicUsize,
    pub(super) stop_count: std::sync::atomic::AtomicUsize,
    stop_error: Mutex<Option<crate::error::RecorderError>>,
    audio_level_cb: Mutex<Option<AudioLevelCallback>>,
}

impl FakeRecorder {
    pub(super) fn new(audio: Vec<u8>) -> Self {
        Self {
            recording: std::sync::atomic::AtomicBool::new(false),
            audio,
            start_count: std::sync::atomic::AtomicUsize::new(0),
            stop_count: std::sync::atomic::AtomicUsize::new(0),
            stop_error: Mutex::new(None),
            audio_level_cb: Mutex::new(None),
        }
    }

    pub(super) fn with_stop_error(error: crate::error::RecorderError, audio: Vec<u8>) -> Self {
        Self {
            recording: std::sync::atomic::AtomicBool::new(false),
            audio,
            start_count: std::sync::atomic::AtomicUsize::new(0),
            stop_count: std::sync::atomic::AtomicUsize::new(0),
            stop_error: Mutex::new(Some(error)),
            audio_level_cb: Mutex::new(None),
        }
    }

    pub(super) fn start_count(&self) -> usize {
        self.start_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(super) fn stop_count(&self) -> usize {
        self.stop_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Recorder for FakeRecorder {
    fn start_recording(&mut self) -> Result<(), crate::error::RecorderError> {
        self.recording
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.start_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    fn stop_recording(&self) -> Result<Vec<u8>, crate::error::RecorderError> {
        self.recording
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.stop_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(err) = self.stop_error.lock().unwrap().take() {
            return Err(err);
        }
        Ok(self.audio.clone())
    }
    fn is_recording(&self) -> bool {
        self.recording.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn set_audio_level_callback(&mut self, callback: Option<AudioLevelCallback>) {
        *self.audio_level_cb.lock().unwrap() = callback;
    }
}

impl Recorder for std::sync::Arc<FakeRecorder> {
    fn start_recording(&mut self) -> Result<(), crate::error::RecorderError> {
        let ptr: *mut FakeRecorder = Arc::as_ptr(self) as *mut _;
        unsafe { (*ptr).start_recording() }
    }
    fn stop_recording(&self) -> Result<Vec<u8>, crate::error::RecorderError> {
        (**self).stop_recording()
    }
    fn is_recording(&self) -> bool {
        (**self).is_recording()
    }
    fn set_audio_level_callback(&mut self, callback: Option<AudioLevelCallback>) {
        let ptr: *mut FakeRecorder = Arc::as_ptr(self) as *mut _;
        unsafe { (*ptr).set_audio_level_callback(callback) }
    }
}

// ---------------------------------------------------------------------------
// Transcriber fake（假转写器）
// ---------------------------------------------------------------------------

pub(super) struct FakeTranscriber {
    outcome:
        Mutex<Option<Result<crate::transcriber::TranscribeResult, crate::error::TranscriberError>>>,
    pub(super) call_count: std::sync::atomic::AtomicUsize,
}

impl FakeTranscriber {
    pub(super) fn new(
        outcome: Result<crate::transcriber::TranscribeResult, crate::error::TranscriberError>,
    ) -> Self {
        Self {
            outcome: Mutex::new(Some(outcome)),
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub(super) fn with_success(text: &str, language: &str) -> Self {
        Self::new(Ok(crate::transcriber::TranscribeResult {
            text: text.to_string(),
            language: language.to_string(),
        }))
    }

    pub(super) fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 重写下一次转写的结果，供“失败后重试成功”的流程测试使用。
    pub(super) fn set_outcome(
        &self,
        outcome: Result<crate::transcriber::TranscribeResult, crate::error::TranscriberError>,
    ) {
        *self.outcome.lock().unwrap() = Some(outcome);
    }
}

impl Transcriber for FakeTranscriber {
    fn transcribe<'life0, 'life1>(
        &'life0 self,
        _audio: &'life1 [u8],
        _on_progress: Arc<dyn Fn(f32) + Send + Sync>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        crate::transcriber::TranscribeResult,
                        crate::error::TranscriberError,
                    >,
                > + Send
                + 'life0,
        >,
    >
    where
        'life1: 'life0,
    {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let result = self
            .outcome
            .lock()
            .unwrap()
            .take()
            .expect("FakeTranscriber::transcribe called more than once");
        Box::pin(async move { result })
    }
}

impl Transcriber for std::sync::Arc<FakeTranscriber> {
    fn transcribe<'life0, 'life1>(
        &'life0 self,
        audio: &'life1 [u8],
        on_progress: Arc<dyn Fn(f32) + Send + Sync>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        crate::transcriber::TranscribeResult,
                        crate::error::TranscriberError,
                    >,
                > + Send
                + 'life0,
        >,
    >
    where
        'life1: 'life0,
    {
        std::sync::Arc::as_ref(self).transcribe(audio, on_progress)
    }
}

// ---------------------------------------------------------------------------
// PipelineSink mock：记录状态变化、错误、结果以便断言
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(super) struct MockSink {
    status_changes: Arc<Mutex<Vec<PipelineStatus>>>,
    errors: Arc<Mutex<Vec<UserFacingError>>>,
    results: Arc<Mutex<Vec<TranscriptionResult>>>,
    progress: ProgressEvents,
    pending: Arc<Mutex<Vec<Option<super::pending::PendingRecordingInfo>>>>,
}

impl MockSink {
    pub(super) fn new() -> Self {
        Self {
            status_changes: Arc::new(Mutex::new(Vec::new())),
            errors: Arc::new(Mutex::new(Vec::new())),
            results: Arc::new(Mutex::new(Vec::new())),
            progress: Arc::new(Mutex::new(Vec::new())),
            pending: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(super) fn status_changes(&self) -> Vec<PipelineStatus> {
        self.status_changes.lock().unwrap().clone()
    }

    pub(super) fn errors(&self) -> Vec<UserFacingError> {
        self.errors.lock().unwrap().clone()
    }

    pub(super) fn results(&self) -> Vec<TranscriptionResult> {
        self.results.lock().unwrap().clone()
    }

    pub(super) fn progress(&self) -> Vec<ProgressEvent> {
        self.progress.lock().unwrap().clone()
    }

    pub(super) fn pending_events(&self) -> Vec<Option<super::pending::PendingRecordingInfo>> {
        self.pending.lock().unwrap().clone()
    }
}

impl PipelineSink for MockSink {
    fn on_status_change(&self, status: PipelineStatus) {
        self.status_changes.lock().unwrap().push(status);
    }
    fn on_error(&self, error: &UserFacingError) {
        self.errors.lock().unwrap().push(error.clone());
    }
    fn on_transcription_result(&self, output: &TranscriptionResult) {
        self.results.lock().unwrap().push(output.clone());
    }
    fn on_progress(&self, phase: &str, fraction: Option<f32>) {
        self.progress
            .lock()
            .unwrap()
            .push((phase.to_string(), fraction));
    }
    fn on_key_listener_backend(&self, _: &str) {}
    fn on_pending_recording(&self, pending: Option<&super::pending::PendingRecordingInfo>) {
        self.pending.lock().unwrap().push(pending.cloned());
    }
}

// ---------------------------------------------------------------------------
// Output fake：记录剪贴板写入与文本注入
// ---------------------------------------------------------------------------

/// 共享的调用记录句柄（剪贴板写入 / 文本注入）。
pub(super) type SharedCallLog = Arc<Mutex<Vec<String>>>;

pub(super) struct FakeOutput {
    pub(super) clipboard_writes: SharedCallLog,
    pub(super) injections: SharedCallLog,
}

impl FakeOutput {
    pub(super) fn new() -> (Self, SharedCallLog, SharedCallLog) {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let injections = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                clipboard_writes: Arc::clone(&writes),
                injections: Arc::clone(&injections),
            },
            writes,
            injections,
        )
    }
}

impl Output for FakeOutput {
    fn write_clipboard(&self, text: &str) -> Result<(), OutputError> {
        self.clipboard_writes.lock().unwrap().push(text.to_string());
        Ok(())
    }

    fn inject_text(&self, text: &str) -> Result<(), OutputError> {
        self.injections.lock().unwrap().push(text.to_string());
        Ok(())
    }

    fn clone_box(&self) -> Arc<dyn Output> {
        Arc::new(FakeOutput {
            clipboard_writes: Arc::clone(&self.clipboard_writes),
            injections: Arc::clone(&self.injections),
        })
    }
}
