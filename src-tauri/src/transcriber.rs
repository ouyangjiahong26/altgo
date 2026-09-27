//! 语音识别模块。
//!
//! `Transcriber` trait 抽象本地语音识别后端；当前生产实现为
//! `SherpaTranscriber`（`crate::sherpa`）内嵌 sherpa-onnx 的 SenseVoice。
//!
//! 所有实现都返回 `TranscribeResult`（文本 + 语言信息），进度通过闭包回调上报。

use crate::error::TranscriberError;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// 语音识别结果。
#[derive(Debug)]
pub struct TranscribeResult {
    /// 识别出的文本
    pub text: String,
    /// 检测到的语言代码
    pub language: String,
}

/// 统一的转写后端 trait——`on_progress` 回调由调用方提供，trait 表面不携带
/// 通道类型，新后端接入无需改动 trait。
pub trait Transcriber: Send + Sync {
    /// 转写 WAV 音频数据。
    ///
    /// `on_progress` 收到 0.0–1.0 的进度；无流式进度的后端成功时也应调用一次
    /// `1.0`，让 UI 收到最终一帧。
    fn transcribe<'life0, 'life1>(
        &'life0 self,
        audio: &'life1 [u8],
        on_progress: Arc<dyn Fn(f32) + Send + Sync>,
    ) -> Pin<Box<dyn Future<Output = Result<TranscribeResult, TranscriberError>> + Send + 'life0>>
    where
        'life1: 'life0;
}
