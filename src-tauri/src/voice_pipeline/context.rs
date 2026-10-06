//! PipelineContext：拥有所有组件并运行事件循环。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::error::UserFacingError;
use crate::key_listener::KeyListener;
use crate::polisher::{LLMFormatter, PolishLevel};
use crate::recorder::Recorder;
use crate::state_machine::{Command, Machine};
use crate::transcriber::Transcriber;

use super::handlers::{handle_start_record, handle_stop_record, transcribe_and_dispatch};
use super::pending::PendingRecordingStore;
use super::sink::PipelineSink;
use crate::pipeline_controller::PipelineStatus;

/// 流水线运行期间全部组件的持有者。
pub struct PipelineContext {
    pub(crate) recorder: Box<dyn Recorder>,
    pub(crate) transcriber: Box<dyn Transcriber>,
    pub(crate) formatter: LLMFormatter,
    pub(crate) polish_level: PolishLevel,
    pub(crate) listener: Mutex<Option<Box<dyn KeyListener>>>,
    /// 待重试录音槽位：转写失败时保留录音本体，重试请求到达时取出再识别。
    pub(crate) pending_store: PendingRecordingStore,
    /// 重试请求通道：IPC 命令经 `RetryRequestHandle` 发送，主循环串行消费，
    /// 维持 ADR-0003 的单次转写互斥。
    pub(crate) retry_rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    // 状态机参数
    pub(crate) long_press_threshold: std::time::Duration,
    pub(crate) double_click_interval: std::time::Duration,
    pub(crate) min_press_duration: std::time::Duration,
}

impl PipelineContext {
    /// 运行流水线事件循环，直到 `stop_rx` 触发。
    pub async fn run(self, stop_rx: tokio::sync::oneshot::Receiver<()>, sink: impl PipelineSink) {
        let mut recorder = self.recorder;
        let transcriber = self.transcriber;
        let formatter = self.formatter;
        let polish_level = self.polish_level;
        let pending_store = self.pending_store;
        let mut retry_rx = self.retry_rx;
        // 把 sink 包进 Arc，让 handler 在异步任务生命周期结束后仍能使用，
        // 进度转发器也需要持有它。
        let sink: Arc<dyn PipelineSink> = Arc::new(sink);

        // 为录音器配置音频电平回调，把实时计算的 RMS 电平推入 sink
        let level_sink = sink.clone();
        recorder.set_audio_level_callback(Some(Arc::new(move |level: f32| {
            level_sink.on_audio_level(level);
        })));

        let mut listener: Box<dyn KeyListener> = match self.listener.lock().unwrap().take() {
            Some(l) => l,
            None => {
                sink.on_error(&UserFacingError {
                    code: "internal.context_already_used".into(),
                    params: None,
                });
                return;
            }
        };

        let (mut key_events, key_backend): (
            tokio::sync::mpsc::UnboundedReceiver<crate::key_listener::KeyEvent>,
            &'static str,
        ) = match listener.start() {
            Ok(pair) => pair,
            Err(e) => {
                // 此处拿不到 backend 名，不构造 FatalError::KeyListenerFailed。
                sink.on_error(&UserFacingError {
                    code: "fatal.key_listener_start_failed".into(),
                    params: Some(BTreeMap::from([("detail".to_string(), e.to_string())])),
                });
                return;
            }
        };
        tracing::info!(backend = key_backend, "key listener active");
        sink.on_key_listener_backend(key_backend);

        // 创建状态机，直接集成到主循环
        let mut machine = Machine::new(
            self.long_press_threshold,
            self.double_click_interval,
            self.min_press_duration,
        );
        let mut deadline: Option<tokio::time::Instant> = None;

        sink.on_status_change(PipelineStatus::Idle);

        let mut stop_rx = stop_rx;

        /// 主循环本轮要处理的事：状态机命令或一次待重试录音的重新识别。
        enum Step {
            Command(Option<Command>),
            Retry,
        }

        loop {
            let step = tokio::select! {
                // 按键事件
                event = key_events.recv() => Step::Command(match event {
                    Some(ev) => machine.process(ev),
                    None => {
                        tracing::warn!("key event channel closed, stopping pipeline");
                        break;
                    }
                }),
                // 超时事件
                _ = async { tokio::time::sleep_until(deadline.unwrap()).await }, if deadline.is_some() => {
                    Step::Command(machine.poll_timeout())
                }
                // 重试待重试录音：与按键触发的转写在同一循环内串行执行。
                Some(()) = retry_rx.recv() => Step::Retry,
                // 停止信号
                _ = &mut stop_rx => {
                    tracing::info!("pipeline stop requested");
                    break;
                }
            };

            match step {
                Step::Command(cmd) => {
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
                                    &pending_store,
                                    sink.clone(),
                                )
                                .await;
                            }
                        }
                    }
                }
                Step::Retry => {
                    if machine.is_recording() {
                        // 录音优先：此刻转写会把录音中的悬浮窗切成转写中再隐藏，
                        // 音量事件流也会被掐断。请求丢弃，用户空闲后再点即可。
                        tracing::warn!("retry requested during recording, ignored until idle");
                    } else {
                        match pending_store.take_wav() {
                            Some(wav) => {
                                tracing::info!("retrying pending transcription");
                                let ok = transcribe_and_dispatch(
                                    &wav,
                                    &*transcriber,
                                    &formatter,
                                    polish_level,
                                    &pending_store,
                                    sink.clone(),
                                )
                                .await;
                                if ok {
                                    // 槽位已消费，告知前端横幅可以撤下。
                                    sink.on_pending_recording(None);
                                }
                            }
                            None => {
                                tracing::warn!("retry requested but no pending recording");
                            }
                        }
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
// 测试
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
        let (_retry_tx, retry_rx) = tokio::sync::mpsc::unbounded_channel();
        build_test_context(
            listener,
            Box::new(PlatformRecorder::new(16000)),
            Box::new(super::super::test_doubles::FakeTranscriber::with_success(
                "hello", "en",
            )),
            PolishLevel::None,
            LLMFormatter::from_config(&test_polisher_config(), "en").unwrap(),
            std::time::Duration::from_millis(400),
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(80),
            PendingRecordingStore::default(),
            retry_rx,
        )
    }

    fn make_test_wav() -> Vec<u8> {
        // 1000 ms：高于 300 ms 误触守卫，代表一段真实录音。
        let samples: Vec<u8> = (0..16000).flat_map(|_| 0i16.to_le_bytes()).collect();
        crate::audio::encode_wav(&samples, 16000, 1, 16).unwrap()
    }

    fn make_test_context(
        listener: Box<dyn KeyListener>,
        recorder: Box<dyn Recorder>,
        transcriber: Box<dyn Transcriber>,
        polish_level: PolishLevel,
    ) -> PipelineContext {
        let (_retry_tx, retry_rx) = tokio::sync::mpsc::unbounded_channel();
        make_test_context_with_retry(
            listener,
            recorder,
            transcriber,
            polish_level,
            PendingRecordingStore::default(),
            retry_rx,
        )
    }

    fn make_test_context_with_retry(
        listener: Box<dyn KeyListener>,
        recorder: Box<dyn Recorder>,
        transcriber: Box<dyn Transcriber>,
        polish_level: PolishLevel,
        pending_store: PendingRecordingStore,
        retry_rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    ) -> PipelineContext {
        build_test_context(
            Some(listener),
            recorder,
            transcriber,
            polish_level,
            LLMFormatter::new(
                "test-key".to_string(),
                "http://127.0.0.1:1".to_string(),
                "test-model".to_string(),
                std::time::Duration::from_millis(1),
            )
            .unwrap(),
            std::time::Duration::from_millis(60),
            std::time::Duration::from_millis(60),
            std::time::Duration::from_millis(10),
            pending_store,
            retry_rx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_test_context(
        listener: Option<Box<dyn KeyListener>>,
        recorder: Box<dyn Recorder>,
        transcriber: Box<dyn Transcriber>,
        polish_level: PolishLevel,
        formatter: LLMFormatter,
        long_press_threshold: std::time::Duration,
        double_click_interval: std::time::Duration,
        min_press_duration: std::time::Duration,
        pending_store: PendingRecordingStore,
        retry_rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    ) -> PipelineContext {
        PipelineContext {
            recorder,
            transcriber,
            formatter,
            polish_level,
            listener: Mutex::new(listener),
            pending_store,
            retry_rx,
            long_press_threshold,
            double_click_interval,
            min_press_duration,
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
            fn on_error(&self, _: &UserFacingError) {}
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
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        handle.send(KeyEvent { pressed: false });

        // 等待转写与润色重试结束（每次重试都做退避）。
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
        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        handle.send(KeyEvent { pressed: false });

        // 双击窗口内的第二次按下。
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
    async fn accidental_press_barely_over_threshold_discards_recording_without_transcribing() {
        let (listener, handle) = FakeListener::new("fake");
        // 20 ms 音频：长按阈值刚过就松开的意外按压只能录到一瞬音频。
        let samples: Vec<u8> = (0..320).flat_map(|_| 0i16.to_le_bytes()).collect();
        let wav = crate::audio::encode_wav(&samples, 16000, 1, 16).unwrap();
        let recorder = Arc::new(FakeRecorder::new(wav));
        let transcriber = Arc::new(FakeTranscriber::with_success("accidental", "en"));
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

        // 按住 80 ms：超过 60 ms 长按阈值（StartRecord 已发出），随即松开。
        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        handle.send(KeyEvent { pressed: false });

        // 等双击窗口结束，让状态机回到稳态。
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let _ = stop_tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), run_handle).await;

        assert_eq!(recorder.stop_count(), 1, "录音器照常停止");
        assert_eq!(
            transcriber.call_count(),
            0,
            "过短录音不得进入转写（误触不该触发网络回合）"
        );
        assert!(
            !sink.status_changes().contains(&PipelineStatus::Processing),
            "误触不得展示转写中转圈"
        );
        assert!(sink.results().is_empty());
        assert!(sink.errors().is_empty());
    }

    #[tokio::test]
    async fn failed_transcription_retains_recording_for_retry() {
        let (listener, handle) = FakeListener::new("fake");
        let recorder = Arc::new(FakeRecorder::new(make_test_wav()));
        let transcriber = Arc::new(FakeTranscriber::new(Err(
            crate::error::TranscriberError::HttpError("service unavailable".to_string()),
        )));
        let pending_store = PendingRecordingStore::default();
        let ctx = make_test_context_with_retry(
            Box::new(listener),
            Box::new(Arc::clone(&recorder)),
            Box::new(Arc::clone(&transcriber)),
            PolishLevel::None,
            pending_store.clone(),
            tokio::sync::mpsc::unbounded_channel().1,
        );
        let sink = MockSink::new();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

        let run_handle = tokio::spawn(ctx.run(stop_rx, sink.clone()));
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        handle.send(KeyEvent { pressed: false });

        // 等失败路径走完（录音进待重试槽位，初始 Idle 不代表完成）。
        for _ in 0..100 {
            if pending_store.peek_info().is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let _ = stop_tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), run_handle).await;

        // MiMo 失效后录音本体必须留在待重试槽位，而不是随失败丢弃。
        let info = pending_store.peek_info().expect("失败后应保留录音本体");
        assert_eq!(info.duration_ms, 1000);
        assert_eq!(info.error.code, "transcriber.http_error");
        assert_eq!(sink.pending_events().len(), 1);
        assert!(!sink.errors().is_empty());
    }

    #[tokio::test]
    async fn retry_request_retranscribes_pending_recording() {
        let (listener, handle) = FakeListener::new("fake");
        let recorder = Arc::new(FakeRecorder::new(make_test_wav()));
        let transcriber = Arc::new(FakeTranscriber::new(Err(
            crate::error::TranscriberError::HttpError("service unavailable".to_string()),
        )));
        let pending_store = PendingRecordingStore::default();
        let (retry_tx, retry_rx) = tokio::sync::mpsc::unbounded_channel();
        let ctx = make_test_context_with_retry(
            Box::new(listener),
            Box::new(Arc::clone(&recorder)),
            Box::new(Arc::clone(&transcriber)),
            PolishLevel::None,
            pending_store.clone(),
            retry_rx,
        );
        let sink = MockSink::new();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

        let run_handle = tokio::spawn(ctx.run(stop_rx, sink.clone()));
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        // 第一次录音：识别失败，录音进待重试槽位。
        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        handle.send(KeyEvent { pressed: false });
        for _ in 0..100 {
            if pending_store.peek_info().is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            pending_store.peek_info().is_some(),
            "前置：失败后录音已保留"
        );

        // 服务恢复：换上成功结果并发起重试请求。
        transcriber.set_outcome(Ok(crate::transcriber::TranscribeResult {
            text: "recovered text".to_string(),
            language: "zh".to_string(),
        }));
        retry_tx.send(()).unwrap();

        for _ in 0..100 {
            if !sink.results().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let _ = stop_tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), run_handle).await;

        assert_eq!(transcriber.call_count(), 2, "重试必须重新识别同一段音频");
        let results = sink.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].raw_text, "recovered text");
        assert!(pending_store.peek_info().is_none(), "重试成功后槽位清空");
        assert_eq!(
            sink.pending_events().last(),
            Some(&None),
            "前端应收到撤下横幅事件"
        );
    }

    #[tokio::test]
    async fn retry_request_during_recording_is_ignored() {
        let (listener, handle) = FakeListener::new("fake");
        let recorder = Arc::new(FakeRecorder::new(make_test_wav()));
        let transcriber = Arc::new(FakeTranscriber::new(Err(
            crate::error::TranscriberError::HttpError("service unavailable".to_string()),
        )));
        let pending_store = PendingRecordingStore::default();
        let (retry_tx, retry_rx) = tokio::sync::mpsc::unbounded_channel();
        let ctx = make_test_context_with_retry(
            Box::new(listener),
            Box::new(Arc::clone(&recorder)),
            Box::new(Arc::clone(&transcriber)),
            PolishLevel::None,
            pending_store.clone(),
            retry_rx,
        );
        let sink = MockSink::new();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

        let run_handle = tokio::spawn(ctx.run(stop_rx, sink.clone()));
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        // 第一次录音失败，录音进待重试槽位。
        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        handle.send(KeyEvent { pressed: false });
        for _ in 0..100 {
            if pending_store.peek_info().is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(pending_store.peek_info().is_some(), "前置：录音已保留");

        // 开始新一轮录音并在录音中发重试请求：必须被忽略。
        handle.send(KeyEvent { pressed: true });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert!(recorder.is_recording(), "前置：处于录音中");

        retry_tx.send(()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        assert_eq!(transcriber.call_count(), 1, "录音中的重试请求不得触发转写");
        assert!(sink.results().is_empty());
        assert!(pending_store.peek_info().is_some(), "录音本体仍留在槽位");

        let _ = stop_tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), run_handle).await;
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
