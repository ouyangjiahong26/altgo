//! 命令处理器与结果处理。
//!
//! `handle_start_record` / `handle_stop_record` 是按状态机命令调用的纯业务逻辑。
//! `transcribe_and_dispatch` 是停止录音与重试待重试录音共用的转写收尾；
//! `process_transcription_result` 处理转写完成后的剪贴板写入和历史追加。

use std::sync::Arc;

use crate::error::{TranscriberError, UserFacingError};
use crate::history::HistoryStore;
use crate::output::Output;
use crate::polisher::{LLMFormatter, PolishLevel};
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;

use super::pending::PendingRecordingStore;
use super::sink::{DispatchOutcome, PipelineSink, TranscriptionResult};
use crate::pipeline_controller::PipelineStatus;

/// 最短有效录音时长（毫秒）。低于此值视为误触：长按阈值刚过就松开的意外
/// 按压只能录到一瞬音频，不该触发一次转写回合（在线后端还要为此付一次
/// 网络往返，服务异常时浮窗会转圈到超时）。
pub(crate) const MIN_RECORDING_DURATION_MS: u64 = 300;

/// 处理 StartRecord 命令：开始录音并通知 sink。
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

/// 处理 StopRecord 命令：停止录音，过短的误触录音就地丢弃，其余进入转写收尾。
pub async fn handle_stop_record(
    recorder: &mut dyn Recorder,
    transcriber: &dyn Transcriber,
    formatter: &LLMFormatter,
    polish_level: PolishLevel,
    pending: &PendingRecordingStore,
    sink: Arc<dyn PipelineSink>,
) {
    tracing::info!("recording stopped, processing...");
    let wav_data: Vec<u8> = match recorder.stop_recording() {
        Ok(data) => data,
        Err(e) => {
            tracing::error!(error = %e, "failed to stop recording");
            sink.on_status_change(PipelineStatus::Idle);
            return;
        }
    };

    if audio_too_short(&wav_data) {
        tracing::info!(
            "recording below {}ms, discarding as accidental press",
            MIN_RECORDING_DURATION_MS
        );
        sink.on_status_change(PipelineStatus::Idle);
        return;
    }

    transcribe_and_dispatch(
        &wav_data,
        transcriber,
        formatter,
        polish_level,
        pending,
        sink,
    )
    .await;
}

/// 误触判定：时长解析失败（非 WAV）或低于最短有效时长。
fn audio_too_short(wav_data: &[u8]) -> bool {
    crate::audio::wav_duration_ms(wav_data).is_none_or(|ms| ms < MIN_RECORDING_DURATION_MS)
}

/// 转写 + 润色 + 结果分发的共享收尾，停止录音与重试待重试录音共用。
///
/// 返回 `true` 表示产出了转写结果；`false` 表示识别失败，此时录音本体已
/// 保留进 `pending` 供用户重新识别（ADR-0003：调用方保证串行调用）。
pub(crate) async fn transcribe_and_dispatch(
    wav_data: &[u8],
    transcriber: &dyn Transcriber,
    formatter: &LLMFormatter,
    polish_level: PolishLevel,
    pending: &PendingRecordingStore,
    sink: Arc<dyn PipelineSink>,
) -> bool {
    sink.on_status_change(PipelineStatus::Processing);

    sink.on_progress("transcribe", None);

    // 进度回调是同步的，直接转发给 sink。
    let progress_sink = sink.clone();
    let progress_cb: Arc<dyn Fn(f32) + Send + Sync> = Arc::new(move |fr: f32| {
        progress_sink.on_progress("transcribe", Some(fr));
    });

    let transcribe_result = transcriber.transcribe(wav_data, progress_cb).await;
    let result = match transcribe_result {
        Ok(r) => r,
        Err(e) => {
            let user_error = e.user_error();
            tracing::error!(error = %e, "transcription failed, retaining audio for retry");
            retain_failed_recording(wav_data, user_error, pending, &sink);
            return false;
        }
    };

    tracing::info!(text = %result.text, "transcribed");

    if result.text.is_empty() {
        // 有效长度的录音识别出空文本多半是服务端异常——保留录音供重试，
        // 而不是静默吞掉用户刚说完的话。
        let user_error = TranscriberError::EmptyResult.user_error();
        tracing::warn!("empty transcription result, retaining audio for retry");
        retain_failed_recording(wav_data, user_error, pending, &sink);
        return false;
    }

    sink.on_progress("polish", None);

    let mut polish_failed = false;
    let mut polish_error: Option<UserFacingError> = None;
    let raw_text = result.text.clone();
    let polished = match formatter.polish(&raw_text, polish_level).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "polish failed, using raw text");
            polish_failed = true;
            polish_error = Some(e.user_error());
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
    true
}

/// 识别失败收尾：录音本体入待重试槽位，错误经 `on_error` 与
/// `on_pending_recording` 告知前端，状态回 Idle。
fn retain_failed_recording(
    wav_data: &[u8],
    user_error: UserFacingError,
    pending: &PendingRecordingStore,
    sink: &Arc<dyn PipelineSink>,
) {
    let info = pending.retain(wav_data.to_vec(), user_error.clone());
    sink.on_error(&user_error);
    sink.on_pending_recording(Some(&info));
    sink.on_status_change(PipelineStatus::Idle);
}

/// 按偏好设置与润色状态选择要使用的文本。
pub fn select_text(prefer_polished: bool, output: &TranscriptionResult) -> String {
    if prefer_polished && !output.polish_failed && !output.text.trim().is_empty() {
        output.text.clone()
    } else {
        output.raw_text.clone()
    }
}

/// 为已有历史条目编排一次“润色后再持久化”。
///
/// 从 `history` 读入 `id`，对 `raw_text` 执行润色（`extra_instruction` 非空时经
/// `polish_with_instruction` 附带补充要求），经 `polish_entry` 写回。
/// 所有阻塞 I/O 均移入 `spawn_blocking`。返回更新后的 `HistoryEntry`。
pub async fn dispatch_history_polish(
    history: &HistoryStore,
    id: &str,
    formatter: &LLMFormatter,
    polish_level: PolishLevel,
    extra_instruction: Option<&str>,
) -> Result<crate::history::HistoryEntry, String> {
    let store = history.clone();
    let id_owned = id.to_string();
    let entry = tokio::task::spawn_blocking(move || store.get(&id_owned))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "history entry not found".to_string())?;

    // 补充指令 trim 后为空视同未提供，走普通润色路径。
    let extra = extra_instruction.map(str::trim).filter(|s| !s.is_empty());
    let polished = match extra {
        Some(instruction) => formatter
            .polish_with_instruction(&entry.raw_text, polish_level, instruction)
            .await
            .map_err(|e| e.to_string())?,
        None => formatter
            .polish(&entry.raw_text, polish_level)
            .await
            .map_err(|e| e.to_string())?,
    };

    let store = history.clone();
    let id_owned = id.to_string();
    tokio::task::spawn_blocking(move || store.polish_entry(&id_owned, &polished))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// 处理一次转写结果：选择文本、写剪贴板、追加历史。
///
/// `inject_text` 为 `true` 时（仅 Windows 有实现）把选中文本注入到当前
/// 焦点窗口；为 `false` 时输出动作仅剩剪贴板写入。
/// 转写为空时返回 `None`（不做任何动作）。
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
// 测试
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
                Some(UserFacingError {
                    code: "polisher.api_error".to_string(),
                    params: Some(
                        [("status".to_string(), "500".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                })
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
    async fn handle_stop_record_discards_degenerate_wav_without_transcribing() {
        use crate::polisher::LLMFormatter;

        // 只有 44 字节 WAV 头、无音频数据。
        let mut recorder = super::super::test_doubles::FakeRecorder::new(vec![0u8; 44]);
        let transcriber = super::super::test_doubles::FakeTranscriber::with_success("", "zh");
        let formatter = LLMFormatter::new(
            "test-key".to_string(),
            "http://localhost".to_string(),
            "test-model".to_string(),
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        let pending = PendingRecordingStore::default();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::None,
            &pending,
            sink_arc,
        )
        .await;

        assert_eq!(recorder.stop_count(), 1);
        assert_eq!(transcriber.call_count(), 0);
        assert_eq!(sink.status_changes(), vec![PipelineStatus::Idle]);
        assert!(sink.results().is_empty());
        assert!(sink.errors().is_empty());
        assert!(pending.peek_info().is_none(), "误触录音不进待重试槽位");
    }

    #[tokio::test]
    async fn handle_stop_record_discards_recording_below_min_duration() {
        // 299ms < 300ms 守卫：丢弃；恰好 300ms：进入转写。
        for (samples, expect_transcribed) in [(4784usize, false), (4800, true)] {
            let pcm = vec![0u8; samples * 2];
            let wav = audio::encode_wav(&pcm, 16000, 1, 16).unwrap();
            let mut recorder = super::super::test_doubles::FakeRecorder::new(wav);
            let transcriber =
                super::super::test_doubles::FakeTranscriber::with_success("text", "zh");
            let formatter = failing_formatter();
            let pending = PendingRecordingStore::default();
            let sink = super::super::test_doubles::MockSink::new();
            let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

            recorder.start_recording().unwrap();
            handle_stop_record(
                &mut recorder,
                &transcriber,
                &formatter,
                PolishLevel::None,
                &pending,
                sink_arc,
            )
            .await;

            assert_eq!(
                transcriber.call_count(),
                usize::from(expect_transcribed),
                "samples={samples}"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // handle_stop_record 成功/失败路径测试辅助
    // ---------------------------------------------------------------------------

    fn make_test_wav() -> Vec<u8> {
        // 500ms（16kHz 单声道 16 位）：高于 300ms 误触守卫，代表一段真实录音。
        audio::encode_wav(&vec![0u8; 16000], 16000, 1, 16).unwrap()
    }

    fn failing_formatter() -> LLMFormatter {
        // 连接到一个不会响应的地址，让 polish 在超时/重试后失败。
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
        let pending = PendingRecordingStore::default();
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
                &pending,
                sink_arc,
            ),
        )
        .await
        .expect("transcription result must not wait for a retained progress callback");

        assert!(transcriber.progress.lock().unwrap().is_some());
        // 空文本结果按服务异常处理：保留录音、不发 done 进度、不产出结果。
        assert_eq!(
            sink.progress(),
            vec![
                ("transcribe".to_string(), None),
                ("transcribe".to_string(), Some(0.5)),
            ]
        );
        assert!(sink.results().is_empty());
        let info = pending.peek_info().expect("空结果保留录音");
        assert_eq!(info.error.code, "transcription.empty");
    }

    #[tokio::test]
    async fn handle_stop_record_success_chain_falls_back_to_raw_on_polish_failure() {
        let wav = make_test_wav();
        let mut recorder = super::super::test_doubles::FakeRecorder::new(wav);
        let transcriber =
            super::super::test_doubles::FakeTranscriber::with_success("raw text", "zh");
        let formatter = failing_formatter();
        let pending = PendingRecordingStore::default();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::Medium,
            &pending,
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
    async fn handle_stop_record_transcription_failure_retains_audio_for_retry() {
        let wav = make_test_wav();
        let mut recorder = super::super::test_doubles::FakeRecorder::new(wav.clone());
        let err = TranscriberError::ModelLoadFailed {
            reason: "server error".to_string(),
        };
        // 上报的是稳定错误码结构（含 reason 参数），前端按字典翻译。
        let expected_code = "transcriber.model_load_failed";
        let expected_reason = "server error";
        let transcriber = super::super::test_doubles::FakeTranscriber::new(Err(err));
        let formatter = failing_formatter();
        let pending = PendingRecordingStore::default();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::Medium,
            &pending,
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
        assert_eq!(sink.errors()[0].code, expected_code);
        assert_eq!(
            sink.errors()[0].params.as_ref().unwrap().get("reason"),
            Some(&expected_reason.to_string())
        );
        assert!(sink.results().is_empty());

        // 录音本体必须保留在待重试槽位，而不是随失败一起丢弃。
        let info = pending.peek_info().expect("失败后录音本体应保留");
        assert_eq!(info.duration_ms, 500);
        assert_eq!(info.error.code, expected_code);
        assert_eq!(pending.take_wav(), Some(wav));
        assert_eq!(sink.pending_events(), vec![Some(info)]);
    }

    #[tokio::test]
    async fn handle_stop_record_empty_text_retains_audio_for_retry() {
        let wav = make_test_wav();
        let mut recorder = super::super::test_doubles::FakeRecorder::new(wav);
        let transcriber = super::super::test_doubles::FakeTranscriber::with_success("", "zh");
        let formatter = failing_formatter();
        let pending = PendingRecordingStore::default();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::Medium,
            &pending,
            sink_arc,
        )
        .await;

        assert_eq!(recorder.stop_count(), 1);
        assert_eq!(transcriber.call_count(), 1);
        assert_eq!(
            sink.status_changes(),
            vec![PipelineStatus::Processing, PipelineStatus::Idle]
        );
        // 有效录音识别出空文本按服务异常处理：保留录音，不产出空结果。
        assert!(sink.results().is_empty());
        let info = pending.peek_info().expect("空结果也应保留录音");
        assert_eq!(info.error.code, "transcription.empty");
        assert_eq!(sink.pending_events().len(), 1);
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
        let pending = PendingRecordingStore::default();
        let sink = super::super::test_doubles::MockSink::new();
        let sink_arc: Arc<dyn PipelineSink> = Arc::new(sink.clone());

        recorder.start_recording().unwrap();

        handle_stop_record(
            &mut recorder,
            &transcriber,
            &formatter,
            PolishLevel::Medium,
            &pending,
            sink_arc,
        )
        .await;

        assert_eq!(recorder.stop_count(), 1);
        assert_eq!(transcriber.call_count(), 0);
        assert_eq!(sink.status_changes(), vec![PipelineStatus::Idle]);
        assert!(sink.results().is_empty());
        assert!(sink.errors().is_empty());
        assert!(pending.peek_info().is_none(), "无录音本体可保留");
    }

    // ---------------------------------------------------------------------------
    // dispatch_history_polish 测试
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn dispatch_history_polish_success_updates_entry() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(temp_dir.path().join("history.json"));
        let entry = store
            .append("原始文本".to_string(), "原始文本".to_string())
            .unwrap();

        // PolishLevel::None 会成功返回原文，适合测编排链路而不过度依赖网络。
        let formatter = failing_formatter();
        let result =
            dispatch_history_polish(&store, &entry.id, &formatter, PolishLevel::None, None).await;

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
            dispatch_history_polish(&store, "missing-id", &formatter, PolishLevel::None, None)
                .await;

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
        let formatter = failing_formatter();
        let result =
            dispatch_history_polish(&store, &entry.id, &formatter, PolishLevel::Medium, None).await;

        assert!(result.is_err());
        assert!(!result.unwrap_err().is_empty());

        // 失败时不应写入历史。
        let fetched = store.get(&entry.id).unwrap().unwrap();
        assert_eq!(fetched.text, "原始文本");
    }
}
