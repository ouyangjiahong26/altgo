//! PipelineContext — 拥有所有组件并运行事件循环。
//!
//! PipelineContext — owns all components and runs the event loop.

use std::sync::{Arc, Mutex};

use crate::key_listener::KeyListener;
use crate::polisher::{LLMFormatter, PolishLevel};
use crate::recorder::Recorder;
use crate::state_machine::{Command, Machine};
use crate::transcriber::Transcriber;

use super::handlers::{handle_start_record, handle_stop_record};
use super::sink::PipelineSink;
use crate::pipeline_controller::PipelineStatus;

/// 流水线运行期间全部组件的持有者。
/// Owns all components needed while the pipeline runs.
pub struct PipelineContext {
    pub(crate) recorder: Box<dyn Recorder>,
    pub(crate) transcriber: Box<dyn Transcriber>,
    pub(crate) formatter: LLMFormatter,
    pub(crate) polish_level: PolishLevel,
    pub(crate) listener: Mutex<Option<Box<dyn KeyListener>>>,
    // 状态机参数
    // State machine parameters
    pub(crate) long_press_threshold: std::time::Duration,
    pub(crate) double_click_interval: std::time::Duration,
    pub(crate) min_press_duration: std::time::Duration,
}

impl PipelineContext {
    /// 运行流水线事件循环，直到 `stop_rx` 触发。
    /// Run the pipeline event loop until `stop_rx` fires.
    pub async fn run(self, stop_rx: tokio::sync::oneshot::Receiver<()>, sink: impl PipelineSink) {
        let mut recorder = self.recorder;
        let transcriber = self.transcriber;
        let formatter = self.formatter;
        let polish_level = self.polish_level;
        // 把 sink 包进 Arc，让 handler 在异步任务生命周期结束后仍能使用，
        // 进度转发器也需要持有它。
        // Wrap the sink in an Arc so handlers can keep using it after the
        // async task lifecycle (and so progress forwarders can hold it).
        let sink: Arc<dyn PipelineSink> = Arc::new(sink);

        // 为录音器配置音频电平回调，把实时计算的 RMS 电平推入 sink
        // Configure the recorder's audio level callback, pushing the realtime RMS level into the sink
        let level_sink = sink.clone();
        recorder.set_audio_level_callback(Some(Arc::new(move |level: f32| {
            level_sink.on_audio_level(level);
        })));

        let mut listener: Box<dyn KeyListener> = match self.listener.lock().unwrap().take() {
            Some(l) => l,
            None => {
                sink.on_error("pipeline context already used");
                return;
            }
        };

        let (mut key_events, key_backend): (
            tokio::sync::mpsc::UnboundedReceiver<crate::key_listener::KeyEvent>,
            &'static str,
        ) = match listener.start() {
            Ok(pair) => pair,
            Err(e) => {
                sink.on_error(&format!("key listener start: {}", e));
                return;
            }
        };
        tracing::info!(backend = key_backend, "key listener active");
        sink.on_key_listener_backend(key_backend);

        // 创建状态机，直接集成到主循环
        // Create the state machine, integrated directly into the main loop
        let mut machine = Machine::new(
            self.long_press_threshold,
            self.double_click_interval,
            self.min_press_duration,
        );
        let mut deadline: Option<tokio::time::Instant> = None;

        sink.on_status_change(PipelineStatus::Idle);

        let mut stop_rx = stop_rx;
        loop {
            let cmd = tokio::select! {
                // 按键事件
                // Key events
                event = key_events.recv() => match event {
                    Some(ev) => machine.process(ev),
                    None => {
                        tracing::warn!("key event channel closed, stopping pipeline");
                        break;
                    }
                },
                // 超时事件
                // Timeout events
                _ = async { tokio::time::sleep_until(deadline.unwrap()).await }, if deadline.is_some() => {
                    machine.poll_timeout()
                }
                // 停止信号
                // Stop signal
                _ = &mut stop_rx => {
                    tracing::info!("pipeline stop requested");
                    break;
                }
            };

            if let Some(cmd) = cmd {
                match cmd {
                    Command::StartRecord => {
                        let _ = handle_start_record(&mut *recorder, &*sink);
                    }
                    Command::StopRecord => {
                        handle_stop_record(
                            &mut *recorder,
                            &*transcriber,
                            &formatter,
                            polish_level,
                            sink.clone(),
                        )
                        .await;
                    }
                }
            }
            deadline = machine.next_deadline().map(|d| d.into());
        }

        sink.on_status_change(PipelineStatus::Stopped);
        tracing::info!("pipeline stopped");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_listener::{KeyEvent, KeyListener};
    use crate::polisher::{LLMFormatter, PolishLevel};
    use crate::recorder::PlatformRecorder;
    use crate::transcriber::Transcriber;

    use super::super::test_doubles::{FakeListener, FakeRecorder, FakeTranscriber, MockSink};

    fn test_polisher_config() -> crate::config::PolisherConfig {
        crate::config::PolisherConfig {
            api_key: "test-key".to_string(),
            api_base_url: "http://localhost".to_string(),
            model: "test-model".to_string(),
            protocol: "openai".to_string(),
            max_tokens: 256,
            temperature: 0.0,
            system_prompt: String::new(),
            timeout: std::time::Duration::from_secs(10),
            level: "none".to_string(),
            thinking_level: "off".to_string(),
        }
    }

    fn make_context(listener: Option<Box<dyn KeyListener>>) -> PipelineContext {
        PipelineContext {
            recorder: Box::new(PlatformRecorder::new(16000)),
            transcriber: Box::new(super::super::test_doubles::FakeTranscriber::with_success(
                "hello", "en",
            )),
            formatter: LLMFormatter::from_config(&test_polisher_config(), "en").unwrap(),
            polish_level: PolishLevel::None,
            listener: Mutex::new(listener),
            long_press_threshold: std::time::Duration::from_millis(400),
            double_click_interval: std::time::Duration::from_millis(200),
            min_press_duration: std::time::Duration::from_millis(80),
        }
    }

    fn make_test_wav() -> Vec<u8> {
        let samples: Vec<u8> = (0..1600).flat_map(|_| 0i16.to_le_bytes()).collect();
        crate::audio::encode_wav(&samples, 16000, 1, 16).unwrap()
    }

    fn make_test_context(
        listener: Box<dyn KeyListener>,
        recorder: Box<dyn Recorder>,
        transcriber: Box<dyn Transcriber>,
        polish_level: PolishLevel,
    ) -> PipelineContext {
        PipelineContext {
            recorder,
            transcriber,
            formatter: LLMFormatter::new(
                "test-key".to_string(),
                "http://127.0.0.1:1".to_string(),
                "test-model".to_string(),
                std::time::Duration::from_millis(1),
            )
            .unwrap(),
            polish_level,
            listener: Mutex::new(Some(listener)),
            long_press_threshold: std::time::Duration::from_millis(60),
            double_click_interval: std::time::Duration::from_millis(60),
            min_press_duration: std::time::Duration::from_millis(10),
        }
    }

    #[test]
    fn pipeline_context_accepts_boxed_key_listener() {
        let (fake, _handle) = FakeListener::new("test-fake");
        let fake: Box<dyn KeyListener> = Box::new(fake);
        let ctx = make_context(Some(fake));
        let mut taken = ctx.listener.lock().unwrap().take().unwrap();
        assert_eq!(taken.start().unwrap().1, "test-fake");
    }

    #[test]
    fn pipeline_context_run_returns_early_when_listener_already_taken() {
        struct MockSink;
        impl super::super::sink::PipelineSink for MockSink {
            fn on_status_change(&self, _: crate::pipeline_controller::PipelineStatus) {}
            fn on_error(&self, _: &str) {}
            fn on_transcription_result(&self, _: &super::super::sink::TranscriptionResult) {}
            fn on_progress(&self, _: &str, _: Option<f32>) {}
            fn on_key_listener_backend(&self, _: &str) {}
        }

        let ctx = make_context(None);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        drop(stop_tx);
        rt.block_on(ctx.run(stop_rx, MockSink));
    }

    #[tokio::test]
    async fn long_press_records_transcribes_and_reports_result() {
        let (listener, handle) = FakeListener::new("fake");
        let recorder = Arc::new(FakeRecorder::new(make_test_wav()));
        let transcriber = Arc::new(FakeTranscriber::with_success("raw text", "en"));
        let ctx = make_test_context(
            Box::new(listener),
            Box::new(Arc::clone(&recorder)),
            Box::new(Arc::clone(&transcriber)),
            PolishLevel::Light,
        );
        let sink = MockSink::new();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

        let run_handle = tokio::spawn(ctx.run(stop_rx, sink.clone()));

        // 给事件循环留出进入 select! 的时间。
        // Give the event loop time to enter its select!.
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        handle.send(KeyEvent { pressed: false });

        // 等待转写与润色重试结束（每次重试都做退避）。
        // Wait for transcription + polish retries (each retry backs off).
        for _ in 0..300 {
            if !sink.results().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let _ = stop_tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), run_handle).await;

        assert_eq!(recorder.start_count(), 1);
        assert_eq!(recorder.stop_count(), 1);
        assert_eq!(transcriber.call_count(), 1);

        let results = sink.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].raw_text, "raw text");
        assert_eq!(results[0].text, "raw text");
        assert!(results[0].polish_failed);

        assert_eq!(
            sink.status_changes(),
            vec![
                PipelineStatus::Idle,
                PipelineStatus::Recording,
                PipelineStatus::Processing,
                PipelineStatus::Stopped,
            ]
        );
    }

    #[tokio::test]
    async fn double_click_enters_continuous_recording() {
        let (listener, handle) = FakeListener::new("fake");
        let recorder = Arc::new(FakeRecorder::new(make_test_wav()));
        let transcriber = Arc::new(FakeTranscriber::with_success("continuous raw", "en"));
        let ctx = make_test_context(
            Box::new(listener),
            Box::new(Arc::clone(&recorder)),
            Box::new(Arc::clone(&transcriber)),
            PolishLevel::None,
        );
        let sink = MockSink::new();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

        let run_handle = tokio::spawn(ctx.run(stop_rx, sink.clone()));
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        // 第一次短按。
        // First short press.
        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        handle.send(KeyEvent { pressed: false });

        // 双击窗口内的第二次按下。
        // Second press inside the double-click window.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        handle.send(KeyEvent { pressed: true });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        handle.send(KeyEvent { pressed: false });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        handle.send(KeyEvent { pressed: true });

        for _ in 0..100 {
            if !sink.results().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let _ = stop_tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), run_handle).await;

        assert_eq!(recorder.start_count(), 1);
        assert_eq!(recorder.stop_count(), 1);

        let results = sink.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].raw_text, "continuous raw");
        assert!(!results[0].polish_failed);
    }

    #[tokio::test]
    async fn single_click_shorter_than_min_press_is_ignored() {
        let (listener, handle) = FakeListener::new("fake");
        let recorder = Arc::new(FakeRecorder::new(vec![]));
        let transcriber = Arc::new(FakeTranscriber::with_success("ignored", "en"));
        let ctx = make_test_context(
            Box::new(listener),
            Box::new(Arc::clone(&recorder)),
            Box::new(Arc::clone(&transcriber)),
            PolishLevel::None,
        );
        let sink = MockSink::new();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

        let run_handle = tokio::spawn(ctx.run(stop_rx, sink.clone()));
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        handle.send(KeyEvent { pressed: false });

        // 等待超过双击间隔，让状态机先回到稳态。
        // Wait longer than the double-click interval so the state machine settles.
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;

        let _ = stop_tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), run_handle).await;

        assert_eq!(recorder.start_count(), 0);
        assert_eq!(recorder.stop_count(), 0);
        assert_eq!(transcriber.call_count(), 0);
        assert!(sink.results().is_empty());
        assert_eq!(
            sink.status_changes(),
            vec![PipelineStatus::Idle, PipelineStatus::Stopped]
        );
    }

    #[tokio::test]
    async fn stop_signal_terminates_run() {
        let (listener, _handle) = FakeListener::new("fake");
        let ctx = make_test_context(
            Box::new(listener),
            Box::new(FakeRecorder::new(vec![])),
            Box::new(FakeTranscriber::with_success("", "en")),
            PolishLevel::None,
        );
        let sink = MockSink::new();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

        let run_handle = tokio::spawn(ctx.run(stop_rx, sink.clone()));
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        let _ = stop_tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), run_handle).await;

        assert_eq!(
            sink.status_changes(),
            vec![PipelineStatus::Idle, PipelineStatus::Stopped]
        );
    }
}
