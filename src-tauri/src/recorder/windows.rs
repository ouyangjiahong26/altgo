//! Windows 录音器。
//!
//! 使用 cpal（WASAPI）从默认输入设备采集音频，回调里统一转换为
//! 16kHz 单声道 s16le PCM 写入共享 `Buffer`，输出与 `PulseRecorder` 一致。

use crate::audio::{self, Buffer};
use crate::error::RecorderError;
use crate::recorder::{AudioLevelCallback, Recorder};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// cpal 的 `Stream` 在 Windows 上不是 `Send`（内部持有原生指针），
/// 但 `Recorder` 要求跨线程。这里显式声明发送安全性：WASAPI 流的
/// play/pause/drop 均为线程安全操作，cpal 仅因裸指针未标 `Send`。
struct SendStream(cpal::Stream);
unsafe impl Send for SendStream {}

/// WASAPI 录音器，输出固定为单声道 16kHz s16le（ASR 输入要求）。
pub struct WindowsRecorder {
    sample_rate: u32,
    shared_buffer: Arc<Buffer>,
    recording: Arc<AtomicBool>,
    // Stream 必须保持存活才有数据；stop 时 drop。
    stream: Mutex<Option<SendStream>>,
    audio_level_cb: Arc<RwLock<Option<AudioLevelCallback>>>,
}

impl WindowsRecorder {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            shared_buffer: Arc::new(Buffer::new()),
            recording: Arc::new(AtomicBool::new(false)),
            stream: Mutex::new(None),
            audio_level_cb: Arc::new(RwLock::new(None)),
        }
    }

    fn start(&mut self) -> Result<(), RecorderError> {
        if self.recording.load(Ordering::SeqCst) {
            return Err(RecorderError::StartFailed("already recording".to_string()));
        }

        self.shared_buffer.reset();

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| RecorderError::StartFailed("未找到默认输入音频设备".to_string()))?;

        // 优先请求目标格式；WASAPI 共享模式下系统会自动转换采样率/声道。
        // 若设备拒绝，回退到默认配置，回调里做降采样与混单声道。
        let target_config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = Arc::clone(&self.shared_buffer);
        let recording = Arc::clone(&self.recording);
        let audio_level_cb = Arc::clone(&self.audio_level_cb);
        let sample_rate = self.sample_rate;

        type PcmCallback = Box<dyn FnMut(&[f32], &cpal::InputCallbackInfo) + Send>;

        fn make_callback(
            buffer: Arc<Buffer>,
            recording: Arc<AtomicBool>,
            audio_level_cb: Arc<RwLock<Option<AudioLevelCallback>>>,
            dst_rate: u32,
        ) -> impl Fn(u32, u16) -> PcmCallback {
            move |src_rate: u32, channels: u16| {
                let buffer = Arc::clone(&buffer);
                let recording = Arc::clone(&recording);
                let audio_level_cb = Arc::clone(&audio_level_cb);
                Box::new(move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                    if !recording.load(Ordering::SeqCst) {
                        return;
                    }
                    let pcm = append_pcm(&buffer, data, src_rate, channels, dst_rate);
                    if let Some(cb) = audio_level_cb.read().ok().and_then(|guard| guard.clone()) {
                        if !pcm.is_empty() {
                            let level = audio::calculate_audio_level(&pcm);
                            cb(level);
                        }
                    }
                })
            }
        }

        let make_callback = make_callback(buffer, recording, audio_level_cb, sample_rate);
        let stream = build_stream(&device, &target_config, make_callback(self.sample_rate, 1))
            .or_else(|target_err| {
                tracing::warn!(error = %target_err, "设备拒绝 16kHz 单声道，回退默认格式");
                let default_config = device.default_input_config().map_err(|e| {
                    RecorderError::StartFailed(format!("无法获取默认音频格式: {e}"))
                })?;
                let src_rate = default_config.sample_rate().0;
                let channels = default_config.channels();
                let config: cpal::StreamConfig = default_config.into();
                build_stream(&device, &config, make_callback(src_rate, channels))
            })?;

        stream
            .0
            .play()
            .map_err(|e| RecorderError::StartFailed(format!("启动音频流失败: {e}")))?;

        self.recording.store(true, Ordering::SeqCst);
        *self.stream.lock().expect("stream mutex poisoned") = Some(stream);
        Ok(())
    }

    fn stop(&self) -> Result<Vec<u8>, RecorderError> {
        self.recording.store(false, Ordering::SeqCst);
        // Drop stream 停止采集，回调中最后写入的数据已在 Buffer 里。
        self.stream.lock().expect("stream mutex poisoned").take();

        let pcm_data = self.shared_buffer.read_all();
        if pcm_data.is_empty() {
            return Err(RecorderError::EmptyRecording);
        }
        audio::encode_wav(&pcm_data, self.sample_rate, 1, 16)
            .map_err(|e| RecorderError::CaptureFailed(e.to_string()))
    }
}

/// 按目标采样率构建 f32 输入流。
fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    callback: impl FnMut(&[f32], &cpal::InputCallbackInfo) + Send + 'static,
) -> Result<SendStream, RecorderError> {
    let stream = device
        .build_input_stream(
            config,
            callback,
            |err| tracing::warn!(error = %err, "audio input stream error"),
            None,
        )
        .map_err(|e| RecorderError::StartFailed(format!("构建音频流失败: {e}")))?;
    Ok(SendStream(stream))
}

/// 把 f32 帧转换为 s16le PCM 追加到 `buffer`：先按声道平均混成单声道，
/// 源/目标采样率不同时做线性插值重采样（语音识别够用），返回追加的 PCM 数据。
fn append_pcm(
    buffer: &Buffer,
    data: &[f32],
    src_rate: u32,
    channels: u16,
    dst_rate: u32,
) -> Vec<u8> {
    if channels == 0 || data.len() < channels as usize {
        return Vec::new();
    }
    let frames: Vec<f32> = data
        .chunks(channels as usize)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();

    let step = src_rate as f64 / dst_rate as f64;
    let mut pcm = Vec::with_capacity((frames.len() as f64 / step) as usize * 2);
    let mut pos = 0.0f64;
    while pos + 1.0 < frames.len() as f64 {
        let i = pos.floor() as usize;
        let frac = (pos - i as f64) as f32;
        let sample = frames[i] * (1.0 - frac) + frames[i + 1] * frac;
        let s16 = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        pcm.extend_from_slice(&s16.to_le_bytes());
        pos += step;
    }
    buffer.write(&pcm);
    pcm
}

impl Recorder for WindowsRecorder {
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
    fn append_pcm_passthrough_same_rate() {
        let buf = Buffer::new();
        // 16kHz 单声道，4 个采样；逐帧插值时最后一帧缺右邻，输出 3 帧
        append_pcm(&buf, &[0.0, 0.5, -0.5, 0.25], 16_000, 1, 16_000);
        let pcm = buf.read_all();
        assert_eq!(pcm.len(), 6);
        assert_eq!(i16::from_le_bytes([pcm[2], pcm[3]]), 16383); // 0.5
    }

    #[test]
    fn append_pcm_downsamples_half_rate() {
        let buf = Buffer::new();
        // 32kHz → 16kHz：4 个采样应产出 ~2 个
        append_pcm(&buf, &[0.0, 0.5, -0.5, 0.25], 32_000, 1, 16_000);
        let pcm = buf.read_all();
        assert_eq!(pcm.len(), 4); // 2 个 s16 采样
    }

    #[test]
    fn append_pcm_mixes_stereo() {
        let buf = Buffer::new();
        // 双声道 (L=0.5, R=-0.5) → 单声道 0.0
        append_pcm(&buf, &[0.5, -0.5], 16_000, 2, 16_000);
        let pcm = buf.read_all();
        // 只有 1 帧，pos+1.0 < 1 不成立 → 无输出，但不得 panic
        assert!(pcm.is_empty());
    }

    #[test]
    fn append_pcm_empty_data_noop() {
        let buf = Buffer::new();
        append_pcm(&buf, &[], 16_000, 1, 16_000);
        assert!(buf.read_all().is_empty());
    }
}
