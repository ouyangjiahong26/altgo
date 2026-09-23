//! 转写结果的业务调度 seam。
//!
//! `TauriPipelineSink` 只承担“事件 emit + 浮窗状态切换”，剪贴板写入
//! 与历史追加由 `TranscriptionDispatch` 抽象注入。这是该 seam 的
//! 生产实现和 trait 定义。
//!
//! Business dispatch seam for transcription results.
//!
//! `TauriPipelineSink` only handles event emit and overlay state switching; clipboard writes and
//! history appends are injected through the `TranscriptionDispatch` abstraction. This module holds
//! both the trait definition and its production implementation.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::history::HistoryStore;
use crate::output::Output;

use super::handlers::process_transcription_result;
use super::sink::{DispatchOutcome, TranscriptionResult};

/// 转写结果分发端口：把转写完成事件转为剪贴板写入 + 历史追加，
/// 返回一个描述本次分发结果的 `DispatchOutcome`（若无操作则为 `None`）。
///
/// `TauriPipelineSink` 只持有这个 trait object，不再直接接触
/// `Output` 与 `HistoryStore`。测试可注入 fake，跳过真实业务。
///
/// Dispatch port for finished transcriptions: turns a transcription-completed event into a
/// clipboard write plus a history append, returning a `DispatchOutcome` describing what
/// happened (`None` when there was nothing to do).
///
/// `TauriPipelineSink` only ever holds this trait object, never touching `Output` or
/// `HistoryStore` directly. Tests can inject fakes to skip real business logic.
pub trait TranscriptionDispatch: Send + Sync + 'static {
    fn dispatch<'a>(
        &'a self,
        output: &'a TranscriptionResult,
        prefer_polished: bool,
    ) -> Pin<Box<dyn Future<Output = Option<DispatchOutcome>> + Send + 'a>>;
}

/// 生产实现：把转写结果转发给 `process_transcription_result`。
///
/// `inject_text` 构造时从配置读入（仅 Windows 实现注入；关闭时仅写剪贴板）。
///
/// Production implementation: forwards results to `process_transcription_result`.
///
/// `inject_text` is read from config at construction time (only Windows implements injection;
/// when disabled, only the clipboard is written).
pub struct TranscriptionDispatcherImpl {
    pub output: Arc<dyn Output>,
    pub history_store: HistoryStore,
    pub inject_text: bool,
}

impl TranscriptionDispatch for TranscriptionDispatcherImpl {
    fn dispatch<'a>(
        &'a self,
        output: &'a TranscriptionResult,
        prefer_polished: bool,
    ) -> Pin<Box<dyn Future<Output = Option<DispatchOutcome>> + Send + 'a>> {
        let output_handle = Arc::clone(&self.output);
        let store = self.history_store.clone();
        let inject_text = self.inject_text;
        Box::pin(async move {
            process_transcription_result(
                output,
                prefer_polished,
                inject_text,
                &*output_handle,
                &store,
            )
            .await
        })
    }
}
