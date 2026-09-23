//! 命令处理器与结果处理。
//!
//! `handle_start_record` / `handle_stop_record` 是按状态机命令调用的纯业务逻辑。
//! `process_transcription_result` 处理转写完成后的剪贴板写入和历史追加。
//!
//! Command handlers and result processing.
//!
//! `handle_start_record` / `handle_stop_record` are pure business logic dispatched per state-machine
//! command; `process_transcription_result` handles clipboard writing and history appending once a
//! transcription completes.

use std::sync::Arc;

use crate::history::HistoryStore;
use crate::output::Output;
use crate::polisher::{LLMFormatter, PolishLevel};
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;

use super::sink::{DispatchOutcome, PipelineSink, TranscriptionResult};
use crate::pipeline_controller::PipelineStatus;

/// 处理 StartRecord 命令：开始录音并通知 sink。
/// Handle StartRecord command: start recording and notify sink.
pub fn handle_start_record(
    recorder: &mut dyn Recorder,
    sink: &(impl PipelineSink + ?Sized),
) -> Result<(), String> {
    tracing::info!("recording started");
    recorder
        .start_recording()
        .map_err(|e: crate::error::RecorderError| {
            tracing::error!(error = %e, "failed to start recording");
            e.to_string()
        })?;
    sink.on_status_change(PipelineStatus::Recording);
    Ok(())
}

/// 处理 StopRecord 命令：停止录音、处理音频并通知 sink。
/// Handle StopRecord command: stop recording, process audio, notify sink.
pub async fn handle_stop_record(
    recorder: &mut dyn Recorder,
    transcriber: &dyn Transcriber,
    formatter: &LLMFormatter,
    polish_level: PolishLevel,
    sink: Arc<dyn PipelineSink>,
) {
    tracing::info!("recording stopped, processing...");
    sink.on_status_change(PipelineStatus::Processing);

    let wav_data: Vec<u8> = match recorder.stop_recording() {
        Ok(data) => data,
        Err(e) => {
            tracing::error!(error = %e, "failed to stop recording");
            sink.on_status_change(PipelineStatus::Idle);
            return;
        }
    };

    sink.on_progress("transcribe", None);

    // 进度回调是同步的，直接转发给 sink。
    // Progress callbacks are synchronous, so forward them directly to the sink.
    let progress_sink = sink.clone();
    let progress_cb: Arc<dyn Fn(f32) + Send + Sync> = Arc::new(move |fr: f32| {
        progress_sink.on_progress("transcribe", Some(fr));
    });

    let transcribe_result = transcriber.transcribe(&wav_data, progress_cb).await;
    let result = match transcribe_result {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "transcription failed");
            sink.on_error(&format!("transcription: {}", e));
            sink.on_status_change(PipelineStatus::Idle);
            return;
        }
    };

    tracing::info!(text = %result.text, "transcribed");

    if result.text.is_empty() {
        tracing::warn!("empty transcription, skipping");
        sink.on_progress("done", Some(1.0));
        sink.on_transcription_result(&TranscriptionResult {
            text: String::new(),
            raw_text: String::new(),
            polish_failed: false,
            polish_error: None,
        });
        return;
    }

    sink.on_progress("polish", None);

    let mut polish_failed = false;
    let mut polish_error: Option<String> = None;
    let raw_text = result.text.clone();
    let polished = match formatter.polish(&raw_text, polish_level).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "polish failed, using raw text");
            polish_failed = true;
            polish_error = Some(e.to_string());
            raw_text.clone()
        }
    };

    tracing::info!(text = %polished, "polished");

    sink.on_progress("done", Some(1.0));

    let output = TranscriptionResult {
        text: polished,
        raw_text,
        polish_failed,
        polish_error,
    };
    sink.on_transcription_result(&output);
}

/// 按偏好设置与润色状态选择要使用的文本。
/// Select which text to use based on preferences and polish status.
pub fn select_text(prefer_polished: bool, output: &TranscriptionResult) -> String {
    if prefer_polished && !output.polish_failed && !output.text.trim().is_empty() {
        output.text.clone()
    } else {
        output.raw_text.clone()
    }
}

/// 为已有历史条目编排一次“润色后再持久化”。
///
/// 从 `history` 读入 `id`，对 `raw_text` 执行 `formatter.polish`，经 `polish_entry` 写回。
/// 所有阻塞 I/O 均移入 `spawn_blocking`。返回更新后的 `HistoryEntry`。
/// Dispatch a polish-then-persist pass for an existing history entry.
///
/// Loads `id` from `history`, runs `formatter.polish` on `raw_text`, writes
/// back via `polish_entry`. All blocking I/O is moved to `spawn_blocking`.
/// Returns the updated `HistoryEntry`.
pub async fn dispatch_history_polish(
    history: &HistoryStore,
    id: &str,
    formatter: &LLMFormatter,
    polish_level: PolishLevel,
) -> Result<crate::history::HistoryEntry, String> {
    let store = history.clone();
    let id_owned = id.to_string();
    let entry = tokio::task::spawn_blocking(move || store.get(&id_owned))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "history entry not found".to_string())?;

    let polished = formatter
        .polish(&entry.raw_text, polish_level)
        .await
        .map_err(|e| e.to_string())?;

    let store = history.clone();
    let id_owned = id.to_string();
    tokio::task::spawn_blocking(move || store.polish_entry(&id_owned, &polished))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Process a transcription result: select text, write clipboard, append history.
///
/// `inject_text` 为 `true` 时（仅 Windows 有实现）把选中文本注入到当前
/// 焦点窗口；为 `false` 时输出动作仅剩剪贴板写入。
/// Returns `None` if the transcription was empty (no action taken).
pub async fn process_transcription_result(
    output: &TranscriptionResult,
    prefer_polished: bool,
    inject_text: bool,
    output_adapter: &dyn Output,
    history_store: &HistoryStore,
) -> Option<DispatchOutcome> {
    if output.raw_text.is_empty() {
        return None;
    }

    let text_to_use = select_text(prefer_polished, output);

    // 写剪贴板（阻塞 I/O；调用方已在异步上下文中）
    // Write to clipboard (blocking I/O; caller is already in an async context)
    let text_clone = text_to_use.clone();
    let output_handle = output_adapter.clone_box();
    let clipboard_ok =
        tokio::task::spawn_blocking(move || output_handle.write_clipboard(&text_clone))
            .await
            .ok()
            .and_then(|r| r.ok())
            .is_some();
    if !clipboard_ok {
        tracing::warn!("failed to write clipboard");
    }

    // Windows: 注入到当前焦点窗口；其他平台为 no-op
    // Windows: inject into the currently focused window; other platforms treat it as a no-op
    if inject_text {
        let text_clone = text_to_use.clone();
        let output_handle = output_adapter.clone_box();
        let injected = tokio::task::spawn_blocking(move || output_handle.inject_text(&text_clone))
            .await
            .ok()
            .and_then(|r| r.ok())
            .is_some();
        if !injected {
            tracing::warn!("failed to inject text");
        }
    }

    // 追加历史
    // Append to history
    let raw = output.raw_text.clone();
    let display = text_to_use.clone();
    let store = history_store.clone();
    let history_appended = tokio::task::spawn_blocking(move || store.append(raw, display))
        .await
        .ok()
        .and_then(|r| r.ok())
        .is_some();

    if !history_appended {
        tracing::warn!("failed to append transcription history");
    }

    Some(DispatchOutcome {
        text: text_to_use,
        history_appended,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio;
    use crate::error::{RecorderError, TranscriberError};
    use crate::history::HistoryStore;
    use crate::polisher::{LLMFormatter, PolishLevel};
    use std::sync::Arc;
    use std::time::Duration;

    fn test_output(raw: &str, polished: &str, polish_failed: bool) -> TranscriptionResult {
        TranscriptionResult {
            polish_error: if polish_failed {
                Some("test error".to_string())
            } else {
                None
            },
            raw_text: raw.to_string(),
            text: polished.to_string(),
            polish_failed,
        }
    }

    #[test]
    fn test_select_text_prefer_polished_success() {
        let output = test_output("raw text", "polished text", false);
        assert_eq!(select_text(true, &output), "polished text");
    }

    #[test]
    fn test_select_text_prefer_polished_failed() {
        let output = test_output("raw text", "", true);
        assert_eq!(select_text(true, &output), "raw text");
    }

    #[test]
    fn test_select_text_prefer_polished_empty() {
        let output = test_output("raw text", "  ", false);
        assert_eq!(select_text(true, &output), "raw text");
    }

    #[test]
    fn test_select_text_prefer_raw() {
        let output = test_output("raw text", "polished text", false);
        assert_eq!(select_text(false, &output), "raw text");
    }

    #[tokio::test]
    async fn test_process_transcription_result_empty() {
        use crate::history::HistoryStore;

        let output = test_output("", "", false);
        let (output_adapter, _, _) = super::super::test_doubles::FakeOutput::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let history_store = HistoryStore::new(temp_dir.path().join("history.json"));

        let result =
            process_transcription_result(&output, true, false, &output_adapter, &history_store)
                .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_process_transcription_result_success() {
        use crate::history::HistoryStore;

        let output = test_output("raw text", "polished text", false);
        let (output_adapter, writes, _) = super::super::test_doubles::FakeOutput::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let history_store = HistoryStore::new(temp_dir.path().join("history.json"));

        let result =
            process_transcription_result(&output, true, false, &output_adapter, &history_store)
                .await;
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.text, "polished text");
        assert!(result.history_appended);
        assert_eq!(writes.lock().unwrap().len(), 1);
        assert_eq!(writes.lock().unwrap()[0], "polished text");
    }

    #[tokio::test]
    async fn test_process_transcription_result_inject_disabled_writes_clipboard_only() {
        use crate::history::HistoryStore;

        let output = test_output("raw text", "polished text", false);
        let (output_adapter, writes, injections) = super::super::test_doubles::FakeOutput::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let history_store = HistoryStore::new(temp_dir.path().join("history.json"));

        let result =
            process_transcription_result(&output, true, false, &output_adapter, &history_store)
                .await;

        assert!(result.is_some());
        assert_eq!(writes.lock().unwrap().len(), 1, "剪贴板照常写入");
        assert!(
            injections.lock().unwrap().is_empty(),
            "inject_text 关闭时不得注入文本"
        );
    }

    #[tokio::test]
    async fn test_process_transcription_result_inject_enabled_injects_selected_text() {
        use crate::history::HistoryStore;

        let output = test_output("raw text", "polished text", false);
        let (output_adapter, _, injections) = super::super::test_doubles::FakeOutput::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let history_store = HistoryStore::new(temp_dir.path().join("history.json"));

        let result =
            process_transcription_result(&output, true, true, &output_adapter, &history_store)
                .await;

        assert!(result.is_some());
        assert_eq!(
            injections.lock().unwrap().as_slice(),
            &["polished text".to_string()],
            "注入内容应与剪贴板一致（经 select_text 选择后的文本）"
        );
    }

    #[tokio::test]
    async fn test_process_transcription_result_clipboard_failure_still_returns_result() {
        use crate::history::HistoryStore;

        struct FailingOutput;
        impl crate::output::Output for FailingOutput {
            fn write_clipboard(&self, _text: &str) -> Result<(), crate::error::OutputError> {
                Err(crate::error::OutputError::ClipboardFailed(
                    "no clipboard".to_string(),
                ))
            }
            fn clone_box(&self) -> Arc<dyn crate::output::Output> {
                Arc::new(FailingOutput)
            }
        }

        let output = test_output("raw text", "polished text", false);
        let temp_dir = tempfile::tempdir().unwrap();
        let history_store = HistoryStore::new(temp_dir.path().join("history.json"));

        let result =
            process_transcription_result(&output, true, false, &FailingOutput, &history_store)
                .await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().text, "polished text");
    }

    #[test]
    fn handle_start_record_with_fake_recorder() {
        let mut recorder = super::super::test_doubles::FakeRecorder::new(vec![]);
        let sink = super::super::test_doubles::MockSink::new();
        let result = handle_start_record(&mut recorder, &sink);
        assert!(result.is_ok());
        assert!(recorder.is_recording());
        assert_eq!(sink.status_changes(), vec![PipelineStatus::Recording]);
    }

    #[tokio::test]
    async fn handle_stop_record_with_fake_recorder_reports_empty_audio() {
        use crate::polisher::LLMFormatter;

        let mut recorder = super::super::test_doubles::FakeRecorder::new(vec![0u8; 44]);
        let transcriber = super::super::test_doubles::FakeTranscriber::with_success("", "zh");
        let formatter = LLMFormatter::new(
            "test-key".to_string(),
            "http://localhost".to_string(),
            "test-model".to_string(),
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::None,
            sink_arc,
        )
        .await;

        assert_eq!(recorder.stop_count(), 1);
        assert_eq!(transcriber.call_count(), 1);
        assert_eq!(sink.status_changes(), vec![PipelineStatus::Processing]);
        assert_eq!(sink.results().len(), 1);
        assert!(sink.results()[0].text.is_empty());
        assert!(sink.results()[0].raw_text.is_empty());
        assert!(!sink.results()[0].polish_failed);
        assert!(sink.errors().is_empty());
    }

    // ---------------------------------------------------------------------------
    // Helpers for the handle_stop_record success/failure path tests.
    // handle_stop_record 成功/失败路径测试辅助
    // ---------------------------------------------------------------------------

    fn make_test_wav() -> Vec<u8> {
        let samples: Vec<i16> = vec![0, 1000, -1000, 32767, -32768];
        let mut pcm = Vec::new();
        for s in &samples {
            pcm.extend_from_slice(&s.to_le_bytes());
        }
        audio::encode_wav(&pcm, 16000, 1, 16).unwrap()
    }

    fn failing_formatter() -> LLMFormatter {
        // 连接到一个不会响应的地址，让 polish 在超时/重试后失败。
        // Connect to an address that never responds so polish fails after timeout/retries.
        LLMFormatter::new(
            "test-key".to_string(),
            "http://127.0.0.1:9".to_string(),
            "test-model".to_string(),
            Duration::from_millis(10),
        )
        .unwrap()
    }

    // ---------------------------------------------------------------------------
    // handle_stop_record 成功与失败分支测试
    // Success and failure branch tests for handle_stop_record
    // ---------------------------------------------------------------------------

    type ProgressCallback = Arc<dyn Fn(f32) + Send + Sync>;

    struct RetainingProgressTranscriber {
        progress: std::sync::Mutex<Option<ProgressCallback>>,
    }

    impl RetainingProgressTranscriber {
        fn new() -> Self {
            Self {
                progress: std::sync::Mutex::new(None),
            }
        }
    }

    impl crate::transcriber::Transcriber for RetainingProgressTranscriber {
        fn transcribe<'life0, 'life1>(
            &'life0 self,
            _audio: &'life1 [u8],
            on_progress: Arc<dyn Fn(f32) + Send + Sync>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<crate::transcriber::TranscribeResult, TranscriberError>,
                    > + Send
                    + 'life0,
            >,
        >
        where
            'life1: 'life0,
        {
            on_progress(0.5);
            *self.progress.lock().unwrap() = Some(on_progress);
            Box::pin(async {
                Ok(crate::transcriber::TranscribeResult {
                    text: String::new(),
                    language: "zh".to_string(),
                })
            })
        }
    }

    #[tokio::test]
    async fn handle_stop_record_does_not_wait_for_retained_progress_callback() {
        let wav = make_test_wav();
        let mut recorder = super::super::test_doubles::FakeRecorder::new(wav);
        let transcriber = RetainingProgressTranscriber::new();
        let formatter = failing_formatter();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        tokio::time::timeout(
            Duration::from_secs(1),
            handle_stop_record(
                &mut recorder,
                &transcriber,
                &formatter,
                PolishLevel::None,
                sink_arc,
            ),
        )
        .await
        .expect("transcription result must not wait for a retained progress callback");

        assert!(transcriber.progress.lock().unwrap().is_some());
        assert_eq!(
            sink.progress(),
            vec![
                ("transcribe".to_string(), None),
                ("transcribe".to_string(), Some(0.5)),
                ("done".to_string(), Some(1.0))
            ]
        );
        assert_eq!(sink.results().len(), 1);
    }

    #[tokio::test]
    async fn handle_stop_record_success_chain_falls_back_to_raw_on_polish_failure() {
        let wav = make_test_wav();
        let mut recorder = super::super::test_doubles::FakeRecorder::new(wav);
        let transcriber =
            super::super::test_doubles::FakeTranscriber::with_success("raw text", "zh");
        let formatter = failing_formatter();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::Medium,
            sink_arc,
        )
        .await;

        assert_eq!(recorder.stop_count(), 1);
        assert_eq!(transcriber.call_count(), 1);
        assert_eq!(sink.status_changes(), vec![PipelineStatus::Processing]);

        let results = sink.results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].raw_text, "raw text");
        assert_eq!(results[0].text, "raw text");
        assert!(results[0].polish_failed);
    }

    #[tokio::test]
    async fn handle_stop_record_transcription_failure_emits_error_and_idle() {
        let wav = make_test_wav();
        let mut recorder = super::super::test_doubles::FakeRecorder::new(wav);
        let transcriber = super::super::test_doubles::FakeTranscriber::new(Err(
            TranscriberError::ModelLoadFailed {
                reason: "server error".to_string(),
            },
        ));
        let formatter = failing_formatter();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::Medium,
            sink_arc,
        )
        .await;

        assert_eq!(recorder.stop_count(), 1);
        assert_eq!(transcriber.call_count(), 1);
        assert_eq!(
            sink.status_changes(),
            vec![PipelineStatus::Processing, PipelineStatus::Idle]
        );
        assert!(!sink.errors().is_empty());
        assert!(sink.errors()[0].contains("transcription"));
        assert!(sink.results().is_empty());
    }

    #[tokio::test]
    async fn handle_stop_record_empty_text_emits_empty_result() {
        let wav = make_test_wav();
        let mut recorder = super::super::test_doubles::FakeRecorder::new(wav);
        let transcriber = super::super::test_doubles::FakeTranscriber::with_success("", "zh");
        let formatter = failing_formatter();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::Medium,
            sink_arc,
        )
        .await;

        assert_eq!(recorder.stop_count(), 1);
        assert_eq!(transcriber.call_count(), 1);
        assert_eq!(sink.status_changes(), vec![PipelineStatus::Processing]);

        let results = sink.results();
        assert_eq!(results.len(), 1);
        assert!(results[0].text.is_empty());
        assert!(results[0].raw_text.is_empty());
        assert!(!results[0].polish_failed);
    }

    #[tokio::test]
    async fn handle_stop_record_recorder_failure_returns_idle_without_transcribing() {
        let wav = make_test_wav();
        let mut recorder = super::super::test_doubles::FakeRecorder::with_stop_error(
            RecorderError::StopFailed("device lost".to_string()),
            wav,
        );
        let transcriber =
            super::super::test_doubles::FakeTranscriber::with_success("raw text", "zh");
        let formatter = failing_formatter();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::Medium,
            sink_arc,
        )
        .await;

        assert_eq!(recorder.stop_count(), 1);
        assert_eq!(transcriber.call_count(), 0);
        assert_eq!(
            sink.status_changes(),
            vec![PipelineStatus::Processing, PipelineStatus::Idle]
        );
        assert!(sink.results().is_empty());
        assert!(sink.errors().is_empty());
    }

    // ---------------------------------------------------------------------------
    // dispatch_history_polish 测试
    // dispatch_history_polish tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn dispatch_history_polish_success_updates_entry() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(temp_dir.path().join("history.json"));
        let entry = store
            .append("原始文本".to_string(), "原始文本".to_string())
            .unwrap();

        // PolishLevel::None 会成功返回原文，适合测编排链路而不过度依赖网络。
        // PolishLevel::None returns the original text without network calls—good for testing the
        // orchestration chain without depending on external services.
        let formatter = failing_formatter();
        let result =
            dispatch_history_polish(&store, &entry.id, &formatter, PolishLevel::None).await;

        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.raw_text, "原始文本");
        assert_eq!(updated.text, "原始文本");

        let fetched = store.get(&entry.id).unwrap().unwrap();
        assert_eq!(fetched.text, "原始文本");
        assert_eq!(fetched.raw_text, "原始文本");
    }

    #[tokio::test]
    async fn dispatch_history_polish_missing_entry_returns_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(temp_dir.path().join("history.json"));
        let formatter = failing_formatter();

        let result =
            dispatch_history_polish(&store, "missing-id", &formatter, PolishLevel::None).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn dispatch_history_polish_failure_returns_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(temp_dir.path().join("history.json"));
        let entry = store
            .append("原始文本".to_string(), "原始文本".to_string())
            .unwrap();

        // 使用需要实际调用 API 的级别，让 polish 在连接失败后返回 Err。
        // Use a level that really calls the API so polish returns Err once the connection fails.
        let formatter = failing_formatter();
        let result =
            dispatch_history_polish(&store, &entry.id, &formatter, PolishLevel::Medium).await;

        assert!(result.is_err());
        assert!(!result.unwrap_err().is_empty());

        // 失败时不应写入历史。
        // Failures must not be appended to history.
        let fetched = store.get(&entry.id).unwrap().unwrap();
        assert_eq!(fetched.text, "原始文本");
    }
}
