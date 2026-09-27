//! Linux 录音器。
//!
//! 使用 `parecord` 子进程从默认 PulseAudio 音频源捕获 16 位 PCM 音频（16kHz 单声道）。
//! 在独立线程中运行，通过共享的 `Buffer` 累积音频数据。

use crate::audio::{self, Buffer};
use crate::error::RecorderError;
use crate::recorder::{AudioLevelCallback, Recorder};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

/// PulseAudio 录音器，从默认音频源捕获 16 位 PCM 音频（16kHz 单声道）。
pub struct PulseRecorder {
    sample_rate: u32,
    shared_buffer: Arc<Buffer>,
    recording: Arc<AtomicBool>,
    done: std::sync::Mutex<Option<JoinHandle<()>>>,
    audio_level_cb: Arc<RwLock<Option<AudioLevelCallback>>>,
}

impl PulseRecorder {
    /// 创建新的录音器。输出固定为单声道 16kHz（ASR 输入要求）。
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            shared_buffer: Arc::new(Buffer::new()),
            recording: Arc::new(AtomicBool::new(false)),
            done: std::sync::Mutex::new(None),
            audio_level_cb: Arc::new(RwLock::new(None)),
        }
    }

    /// 开始录音。
    pub fn start(&mut self) -> Result<(), RecorderError> {
        if self.recording.load(Ordering::SeqCst) {
            return Err(RecorderError::StartFailed("already recording".to_string()));
        }

        self.shared_buffer.reset();
        self.recording.store(true, Ordering::SeqCst);

        let sample_rate = self.sample_rate;
        let buffer = Arc::clone(&self.shared_buffer);
        let recording = Arc::clone(&self.recording);
        let audio_level_cb = Arc::clone(&self.audio_level_cb);

        let handle = std::thread::spawn(move || {
            let child = std::process::Command::new("parecord")
                .args([
                    "--format=s16le",
                    &format!("--rate={}", sample_rate),
                    "--channels=1",
                    "--raw",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn();

            let mut child = match child {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %e, "failed to start parecord");
                    recording.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let stdout = match child.stdout.take() {
                Some(s) => s,
                None => {
                    recording.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let mut reader = std::io::BufReader::new(stdout);
            let mut chunk = [0u8; 1024];

            while recording.load(Ordering::SeqCst) {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = &chunk[..n];
                        buffer.write(data);
                        if let Some(cb) = audio_level_cb.read().ok().and_then(|guard| guard.clone())
                        {
                            let level = audio::calculate_audio_level(data);
                            cb(level);
                        }
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "read error in parecord, stopping");
                        break;
                    }
                }
            }

            // 停止 parecord 并把管道里剩余的音频数据取干净。
            let _ = child.kill();
            while let Ok(n) = reader.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                buffer.write(&chunk[..n]);
            }
            let _ = child.wait();
        });

        *self.done.lock().expect("done mutex poisoned") = Some(handle);
        Ok(())
    }

    /// 停止录音并返回 WAV 编码的音频数据。
    pub fn stop(&self) -> Result<Vec<u8>, RecorderError> {
        self.recording.store(false, Ordering::SeqCst);

        // 等待录音线程结束。
        if let Some(handle) = self.done.lock().expect("done mutex poisoned").take() {
            if let Err(e) = handle.join() {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                return Err(RecorderError::StopFailed(format!(
                    "recording thread panicked: {}",
                    msg
                )));
            }
        }

        let pcm_data = self.shared_buffer.read_all();
        if pcm_data.is_empty() {
            return Err(RecorderError::EmptyRecording);
        }

        let wav_data = audio::encode_wav(&pcm_data, self.sample_rate, 1, 16)
            .map_err(|e| RecorderError::CaptureFailed(e.to_string()))?;
        Ok(wav_data)
    }
}

impl Recorder for PulseRecorder {
    fn start_recording(&mut self) -> Result<(), RecorderError> {
        self.start()
    }

    fn stop_recording(&self) -> Result<Vec<u8>, RecorderError> {
        self.stop()
    }

    fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }

    fn set_audio_level_callback(&mut self, callback: Option<AudioLevelCallback>) {
        if let Ok(mut guard) = self.audio_level_cb.write() {
            *guard = callback;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recorder_creation() {
        let recorder = PulseRecorder::new(16000);
        assert_eq!(recorder.sample_rate, 16000);
    }
}
