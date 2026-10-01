//! 待重试录音 — 转写失败后保留的录音本体与其元信息。
//!
//! 保留目的只有一个：用户重新识别。录音只存在内存中、只保留最新一段，
//! 不落盘、不进历史（历史条目永不存音频）；新失败覆盖旧录音。
//!
//! `RetryRequestHandle` 是 IPC 命令到流水线主循环的重试请求通道句柄：
//! 重试在主循环内串行执行，维持 ADR-0003 的单次转写互斥。

use std::sync::{Arc, Mutex};

use crate::error::UserFacingError;

/// 待重试录音变化事件名（Tauri 事件，载荷为 `PendingRecordingInfo` 或 null）。
pub const PENDING_RECORDING_EVENT: &str = "pending-recording-changed";

/// 待重试录音的 IPC 可见元信息（不含音频本体）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRecordingInfo {
    /// 录音时长（毫秒）；WAV 解析失败时为 0。
    pub duration_ms: u64,
    /// 上一次识别失败的原因（错误码结构，前端字典翻译）。
    pub error: UserFacingError,
}

/// 内存中的待重试录音槽位。克隆共享同一槽位。
#[derive(Clone, Default)]
pub struct PendingRecordingStore {
    slot: Arc<Mutex<Option<Slot>>>,
}

struct Slot {
    wav: Vec<u8>,
    info: PendingRecordingInfo,
}

impl PendingRecordingStore {
    /// 保留一段录音（覆盖旧值），返回对外可见的元信息。
    pub fn retain(&self, wav: Vec<u8>, error: UserFacingError) -> PendingRecordingInfo {
        let info = PendingRecordingInfo {
            duration_ms: crate::audio::wav_duration_ms(&wav).unwrap_or(0),
            error,
        };
        *self.lock_slot() = Some(Slot {
            wav,
            info: info.clone(),
        });
        info
    }

    /// 取出录音本体（槽位随之清空）。无待重试录音时返回 `None`。
    pub fn take_wav(&self) -> Option<Vec<u8>> {
        self.lock_slot().take().map(|s| s.wav)
    }

    /// 查看元信息而不清除槽位。
    pub fn peek_info(&self) -> Option<PendingRecordingInfo> {
        self.lock_slot().as_ref().map(|s| s.info.clone())
    }

    /// 清空槽位，返回清空前是否有录音。
    pub fn clear(&self) -> bool {
        self.lock_slot().take().is_some()
    }

    /// 槽位持锁；中毒时沿用既有数据（与 `pipeline_controller` 同一处理方式）。
    fn lock_slot(&self) -> std::sync::MutexGuard<'_, Option<Slot>> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 重试请求通道句柄：流水线每次启动时换入新的 sender。
#[derive(Default)]
pub struct RetryRequestHandle {
    tx: Mutex<Option<tokio::sync::mpsc::UnboundedSender<()>>>,
}

impl RetryRequestHandle {
    /// 换入新的重试请求 sender（流水线重启时调用）。
    pub fn set(&self, tx: tokio::sync::mpsc::UnboundedSender<()>) {
        *self.lock_tx() = Some(tx);
    }

    /// 发送一次重试请求。流水线未运行（receiver 已被丢弃）时返回 `false`。
    pub fn send(&self) -> bool {
        self.lock_tx()
            .as_ref()
            .map(|tx| tx.send(()).is_ok())
            .unwrap_or(false)
    }

    fn lock_tx(&self) -> std::sync::MutexGuard<'_, Option<tokio::sync::mpsc::UnboundedSender<()>>> {
        self.tx.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_with_samples(n: usize) -> Vec<u8> {
        crate::audio::encode_wav(&vec![0u8; n * 2], 16000, 1, 16).unwrap()
    }

    fn bare_error(code: &str) -> UserFacingError {
        UserFacingError {
            code: code.to_string(),
            params: None,
        }
    }

    #[test]
    fn retain_stores_wav_and_computed_info() {
        let store = PendingRecordingStore::default();
        let info = store.retain(
            wav_with_samples(16000),
            bare_error("transcriber.http_error"),
        );
        assert_eq!(info.duration_ms, 1000);
        assert_eq!(info.error.code, "transcriber.http_error");
        assert!(store.peek_info().is_some());
    }

    #[test]
    fn take_wav_returns_audio_and_clears_slot() {
        let store = PendingRecordingStore::default();
        store.retain(wav_with_samples(8000), bare_error("e"));
        assert_eq!(store.take_wav().unwrap().len(), 16000 + 44);
        assert!(store.peek_info().is_none());
        assert!(store.take_wav().is_none());
    }

    #[test]
    fn new_failure_replaces_previous_recording() {
        let store = PendingRecordingStore::default();
        store.retain(wav_with_samples(8000), bare_error("first"));
        store.retain(wav_with_samples(16000), bare_error("second"));
        assert_eq!(store.peek_info().unwrap().error.code, "second");
        assert_eq!(store.take_wav().unwrap().len(), 32000 + 44);
    }

    #[test]
    fn clear_reports_whether_slot_was_occupied() {
        let store = PendingRecordingStore::default();
        assert!(!store.clear());
        store.retain(wav_with_samples(8000), bare_error("e"));
        assert!(store.clear());
        assert!(store.peek_info().is_none());
    }

    #[test]
    fn retry_handle_send_fails_without_receiver() {
        let handle = RetryRequestHandle::default();
        assert!(!handle.send());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        handle.set(tx);
        assert!(handle.send());
        assert_eq!(rx.try_recv(), Ok(()));
        drop(rx);
        assert!(!handle.send(), "receiver 丢弃后发送必须失败");
    }
}
