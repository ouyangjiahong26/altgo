//! Voice Pipeline：拥有完整语音转文字管道。
//!
//! 模块划分：
//! - `sink`：事件接收端口（PipelineSink）与共享类型
//!   （TranscriptionResult / DispatchOutcome）
//! - `dispatcher`：转写结果业务调度 seam（TranscriptionDispatch）
//! - `builder`：PipelineBuilder 组件构造
//! - `context`：PipelineContext 事件循环
//! - `handlers`：命令处理器、结果处理、历史润色编排
//! - `pending`：待重试录音槽位与重试请求通道
//!
//! 公共接口：
//! - `run(cfg, stop_rx, sink, pending_store, retry_rx)`：入口（构建 context + 运行事件循环）
//! - `PipelineBuilder`：单独构造各组件（可测试）
//! - `PipelineContext`：拥有组件，暴露 `run(stop_rx, sink)`
//! - `TranscriptionDispatch` / `TranscriptionDispatcherImpl`：sink 注入的业务 seam
//! - `PendingRecordingStore` / `RetryRequestHandle`：待重试录音与重试请求
//! - `dispatch_history_polish`：历史条目润色编排

mod builder;
mod context;
mod dispatcher;
mod handlers;
pub mod pending;
mod sink;

#[cfg(test)]
mod test_doubles;

pub use builder::PipelineBuilder;
pub use context::PipelineContext;
pub use dispatcher::{TranscriptionDispatch, TranscriptionDispatcherImpl};
pub use handlers::{
    dispatch_history_polish, handle_start_record, handle_stop_record, process_transcription_result,
    select_text,
};
pub use pending::{PendingRecordingInfo, PendingRecordingStore, RetryRequestHandle};
pub use sink::{DispatchOutcome, PipelineSink, TranscriptionResult};

use std::sync::Arc;

/// 端到端运行语音流水线。
///
/// 阻塞当前异步任务直到 `stop_rx` 触发。
/// 所有状态变化与结果均经 `sink` 上报。
pub async fn run(
    cfg: Arc<crate::config::Config>,
    stop_rx: tokio::sync::oneshot::Receiver<()>,
    sink: impl PipelineSink,
    pending_store: PendingRecordingStore,
    retry_rx: tokio::sync::mpsc::UnboundedReceiver<()>,
) {
    let builder = PipelineBuilder::new(cfg.clone());

    let ctx = match builder.build_context(pending_store, retry_rx) {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!(error = %e, "failed to build pipeline context");
            sink.on_error(&e.user_error());
            return;
        }
    };

    ctx.run(stop_rx, sink).await;
}
