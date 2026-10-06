//! 音频模块。
//!
//! 提供线程安全的 PCM 音频缓冲区和 WAV 编码/解码功能。
//!
//! - `Buffer`：基于 `Mutex<Vec<u8>>` 的线程安全字节缓冲区，用于累积录音数据
//! - `encode_wav`：将原始 PCM 数据编码为带 44 字节头的 WAV 格式
//! - `decode_wav_to_f32`：将 16 位 PCM WAV 数据解码为 [-1.0, 1.0] 范围的浮点采样

use crate::error::AudioError;
use std::sync::Mutex;

/// 线程安全的 PCM 音频字节缓冲区。
///
/// 使用 `Mutex<Vec<u8>>` 保护内部数据，支持跨线程并发读写。
pub struct Buffer {
    data: Mutex<Vec<u8>>,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer {
    /// 创建新的空缓冲区。
    pub fn new() -> Self {
        Self {
            data: Mutex::new(Vec::new()),
        }
    }

    /// 以独占访问执行闭包，操作缓冲区数据。
    fn with_lock<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Vec<u8>) -> R,
    {
        let mut data = self.data.lock().expect("audio data mutex poisoned");
        f(&mut data)
    }

    /// 向缓冲区追加数据。
    pub fn write(&self, chunk: &[u8]) {
        self.with_lock(|data| data.extend_from_slice(chunk));
    }

    /// 返回当前缓冲区内容的副本。
    pub fn read_all(&self) -> Vec<u8> {
        self.with_lock(|data| data.clone())
    }

    /// 清空缓冲区。
    pub fn reset(&self) {
        self.with_lock(|data| data.clear());
    }
}

/// 将原始 PCM 数据编码为带 44 字节头的 WAV 格式。
///
/// 参数：`pcm_data`（PCM 字节数据）、`sample_rate`（采样率）、
/// `channels`（声道数）、`bits_per_sample`（位深）。
pub fn encode_wav(
    pcm_data: &[u8],
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
) -> Result<Vec<u8>, AudioError> {
    validate_encode_params(pcm_data, sample_rate, channels, bits_per_sample)?;

    let data_size = pcm_data.len() as u32;
    let mut wav = Vec::with_capacity(44 + pcm_data.len());
    push_wav_header(&mut wav, sample_rate, channels, bits_per_sample, data_size);
    wav.extend_from_slice(pcm_data);

    Ok(wav)
}

/// 编码入参校验：PCM 非空、采样率与声道位深非零，且数据长度能装进 WAV 的 u32 长度字段。
fn validate_encode_params(
    pcm_data: &[u8],
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
) -> Result<(), AudioError> {
    if pcm_data.is_empty() {
        return Err(AudioError::EmptyPcm);
    }
    if sample_rate == 0 {
        return Err(AudioError::InvalidSampleRate);
    }
    if channels == 0 {
        return Err(AudioError::InvalidChannels);
    }
    if bits_per_sample == 0 {
        return Err(AudioError::InvalidBitsPerSample);
    }
    if pcm_data.len() > u32::MAX as usize {
        return Err(AudioError::PcmTooLarge);
    }
    Ok(())
}

/// 拼装 44 字节 WAV 头：RIFF 头、fmt 块与 data 块头（不含数据本身）。
fn push_wav_header(
    wav: &mut Vec<u8>,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    data_size: u32,
) {
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let file_size = 36 + data_size; // RIFF 头 - 8 + 数据

    // RIFF 头
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt 块
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // 块大小
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM 格式
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data 块
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
}

/// 从 16 位 PCM 字节数据中计算感知音频电平（范围 [0.0, 1.0]）。
///
/// 计算 PCM 采样点的均方根（RMS），并通过非线性增益映射到人耳感知的音量电平。
pub fn calculate_audio_level(pcm_data: &[u8]) -> f32 {
    let n_samples = pcm_data.len() / 2;
    if n_samples == 0 {
        return 0.0;
    }

    let mut sum_sq = 0.0f64;
    for i in 0..n_samples {
        let sample = i16::from_le_bytes([pcm_data[i * 2], pcm_data[i * 2 + 1]]) as f64;
        sum_sq += sample * sample;
    }

    let rms = (sum_sq / n_samples as f64).sqrt();
    let ratio = (rms / 32768.0) as f32;

    // 感知增益曲线：放大日常语音幅度，上限截断为 1.0
    const PERCEPTUAL_GAIN: f32 = 3.0;
    (ratio * PERCEPTUAL_GAIN).sqrt().min(1.0)
}

pub fn decode_wav_to_f32(wav_data: &[u8]) -> Result<Vec<f32>, AudioError> {
    if wav_data.len() < 44 {
        return Err(AudioError::WavTooShort);
    }
    if &wav_data[0..4] != b"RIFF" || &wav_data[8..12] != b"WAVE" {
        return Err(AudioError::InvalidWavHeader);
    }

    let (data_offset, data_size) = find_data_chunk(wav_data)?;
    let pcm_data = data_payload(wav_data, data_offset, data_size);

    let n_samples = pcm_data.len() / 2;
    let mut samples = Vec::with_capacity(n_samples);
    for i in 0..n_samples {
        let sample = i16::from_le_bytes([pcm_data[i * 2], pcm_data[i * 2 + 1]]);
        samples.push(sample as f32 / 32768.0);
    }

    Ok(samples)
}

/// 扫描 RIFF 块列表定位 data 块，返回其数据起始偏移与声明长度。
fn find_data_chunk(wav_data: &[u8]) -> Result<(u32, u32), AudioError> {
    let mut offset = 12u32;

    while offset + 8 <= wav_data.len() as u32 {
        let chunk_id = &wav_data[offset as usize..offset as usize + 4];
        let chunk_size = u32::from_le_bytes(
            wav_data[offset as usize + 4..offset as usize + 8]
                .try_into()
                .unwrap(),
        );
        if chunk_id == b"data" {
            return Ok((offset + 8, chunk_size));
        }
        offset += 8 + chunk_size;
        // 对齐到偶数边界。
        if chunk_size % 2 != 0 {
            offset += 1;
        }
    }

    Err(AudioError::MissingDataChunk)
}

/// 取 data 块的 PCM 字节，声明长度超出实际字节时按实际截断。
fn data_payload(wav_data: &[u8], data_offset: u32, data_size: u32) -> &[u8] {
    let end = (data_offset + data_size) as usize;
    if end <= wav_data.len() {
        &wav_data[data_offset as usize..end]
    } else {
        &wav_data[data_offset as usize..]
    }
}

/// 计算 WAV 字节流的音频时长（毫秒）。
///
/// 解析 fmt 块的 byte_rate（每秒字节数）与 data 块长度求时长。data 声明
/// 长度超出实际字节时按实际截断。非 WAV、缺 fmt/data 块或 byte_rate 为 0
/// 时返回 `None`，调用方自行决定回退策略。
pub fn wav_duration_ms(wav_data: &[u8]) -> Option<u64> {
    if wav_data.len() < 44 {
        return None;
    }
    if &wav_data[0..4] != b"RIFF" || &wav_data[8..12] != b"WAVE" {
        return None;
    }

    let mut offset = 12usize;
    let mut byte_rate = None;
    let mut data_size = None;
    while offset + 8 <= wav_data.len() {
        let chunk_id = &wav_data[offset..offset + 4];
        let Ok(size_bytes) = wav_data[offset + 4..offset + 8].try_into() else {
            return None;
        };
        let chunk_size = u32::from_le_bytes(size_bytes) as u64;
        if chunk_id == b"fmt " && offset + 20 <= wav_data.len() {
            // fmt 块数据内偏移 8..12 是 byte_rate（sample_rate × 声道 × 位深 / 8）。
            let Ok(rate_bytes) = wav_data[offset + 16..offset + 20].try_into() else {
                return None;
            };
            byte_rate = Some(u32::from_le_bytes(rate_bytes) as u64);
        } else if chunk_id == b"data" {
            // data 紧跟在块头之后，可用字节不足声明时按实际数量计。
            let available = (wav_data.len() - offset - 8) as u64;
            data_size = Some(chunk_size.min(available));
        }
        offset += 8 + chunk_size as usize;
        // RIFF 块按偶数字节对齐。
        if !chunk_size.is_multiple_of(2) {
            offset += 1;
        }
    }

    let rate = byte_rate?;
    let size = data_size?;
    if rate == 0 {
        return None;
    }
    Some(size * 1000 / rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_buffer_write_and_read() {
        let buf = Buffer::new();
        buf.write(b"hello");
        buf.write(b" world");
        assert_eq!(buf.read_all(), b"hello world");
    }

    #[test]
    fn test_buffer_reset() {
        let buf = Buffer::new();
        buf.write(b"data");
        buf.reset();
        assert!(buf.read_all().is_empty());
    }

    #[test]
    fn test_buffer_read_returns_copy() {
        let buf = Buffer::new();
        buf.write(b"original");
        let copy = buf.read_all();
        buf.reset();
        // 副本应仍包含原始数据。
        assert_eq!(copy, b"original");
    }

    #[test]
    fn test_buffer_concurrent_access() {
        let buf = Arc::new(Buffer::new());
        let mut handles = vec![];

        for i in 0..100 {
            let buf = Arc::clone(&buf);
            handles.push(thread::spawn(move || {
                buf.write(&[i as u8; 10]);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(buf.read_all().len(), 1000);
    }

    #[test]
    fn test_encode_wav_header() {
        let pcm = vec![0u8; 100];
        let wav = encode_wav(&pcm, 16000, 1, 16).unwrap();

        // RIFF 头
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 136); // 36 + 100
        assert_eq!(&wav[8..12], b"WAVE");

        // fmt 块
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u32::from_le_bytes(wav[16..20].try_into().unwrap()), 16); // 块大小
        assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1); // PCM
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1); // 声道数
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16000); // 采样率
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 32000); // 字节率
        assert_eq!(u16::from_le_bytes(wav[32..34].try_into().unwrap()), 2); // 块对齐
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16); // 位深

        // data 块
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 100); // 数据大小
        assert_eq!(&wav[44..], &pcm[..]);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        // 构造采样值已知的 PCM 数据。
        let samples: Vec<i16> = vec![0, 1000, -1000, 32767, -32768];
        let mut pcm = Vec::new();
        for s in &samples {
            pcm.extend_from_slice(&s.to_le_bytes());
        }

        let wav = encode_wav(&pcm, 16000, 1, 16).unwrap();
        let decoded = decode_wav_to_f32(&wav).unwrap();

        assert_eq!(decoded.len(), samples.len());
        for (i, (orig, dec)) in samples.iter().zip(decoded.iter()).enumerate() {
            let expected = *orig as f32 / 32768.0;
            assert!(
                (dec - expected).abs() < 1e-6,
                "sample {i}: expected {expected}, got {dec}"
            );
        }
    }

    #[test]
    fn test_decode_wav_too_short() {
        assert!(decode_wav_to_f32(b"short").is_err());
    }

    #[test]
    fn test_decode_wav_invalid_header() {
        let data = vec![0u8; 100];
        assert!(decode_wav_to_f32(&data).is_err());
    }

    #[test]
    fn test_encode_wav_empty_pcm() {
        assert!(encode_wav(&[], 16000, 1, 16).is_err());
    }

    #[test]
    fn test_encode_wav_zero_sample_rate() {
        assert!(encode_wav(&[0u8; 10], 0, 1, 16).is_err());
    }

    #[test]
    fn test_encode_wav_zero_channels() {
        assert!(encode_wav(&[0u8; 10], 16000, 0, 16).is_err());
    }

    #[test]
    fn test_encode_wav_zero_bits_per_sample() {
        assert!(encode_wav(&[0u8; 10], 16000, 1, 0).is_err());
    }

    #[test]
    fn test_encode_wav_stereo() {
        let pcm = vec![0u8; 100];
        let wav = encode_wav(&pcm, 16000, 2, 16).unwrap();

        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 64000); // 字节率
        assert_eq!(u16::from_le_bytes(wav[32..34].try_into().unwrap()), 4); // 块对齐
    }

    #[test]
    fn test_decode_wav_no_data_chunk() {
        // 只有 fmt 块、没有 data 块的最小 WAV（总长至少 44 字节）。
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&42u32.to_le_bytes()); // 此后的文件大小
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&32000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        // 用 14 字节填充使总长 >= 44 且不含 data 块。
        wav.extend_from_slice(b"junk");
        wav.extend_from_slice(&6u32.to_le_bytes());
        wav.extend_from_slice(&[0x00; 6]);

        assert_eq!(wav.len(), 50);
        assert_eq!(
            decode_wav_to_f32(&wav),
            Err(crate::error::AudioError::MissingDataChunk)
        );
    }

    #[test]
    fn test_decode_wav_truncated_data_chunk() {
        // data 块声称有 100 字节，但后面只有 4 字节。
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&42u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&32000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&100u32.to_le_bytes());
        wav.extend_from_slice(&[0x00, 0x01, 0xFF, 0xFF]);

        let decoded = decode_wav_to_f32(&wav).unwrap();
        // 只有两个完整采样可用。
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn test_decode_wav_odd_chunk_padding() {
        // 在 data 前放一个奇数大小的 'junk' 块，解码器必须跳过填充字节。
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        // 文件大小 = 4 (WAVE) + 8 + 16 (fmt) + 8 + 3 (junk) + 1 (pad) + 8 + 4 (data) = 50
        wav.extend_from_slice(&50u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&32000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"junk");
        wav.extend_from_slice(&3u32.to_le_bytes());
        wav.extend_from_slice(&[0x01, 0x02, 0x03]); // 奇数长度，其后跟填充字节
        wav.push(0x00); // 填充
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&4u32.to_le_bytes());
        wav.extend_from_slice(&0i32.to_le_bytes());

        let decoded = decode_wav_to_f32(&wav).unwrap();
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn test_decode_wav_with_list_chunk() {
        // data 块前面有一个 LIST 块。
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        // 文件大小：WAVE + fmt(24) + LIST(12) + data(8+4) = 52
        wav.extend_from_slice(&52u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&32000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"LIST");
        wav.extend_from_slice(&4u32.to_le_bytes());
        wav.extend_from_slice(b"adtl");
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&4u32.to_le_bytes());
        wav.extend_from_slice(&0i32.to_le_bytes());

        let decoded = decode_wav_to_f32(&wav).unwrap();
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn test_calculate_audio_level_silence() {
        let pcm = vec![0u8; 1024];
        let level = calculate_audio_level(&pcm);
        assert_eq!(level, 0.0);
    }

    #[test]
    fn test_calculate_audio_level_empty() {
        assert_eq!(calculate_audio_level(&[]), 0.0);
    }

    #[test]
    fn test_calculate_audio_level_max_amplitude() {
        // i16 的最大正值是 32767
        let mut pcm = Vec::new();
        for _ in 0..512 {
            pcm.extend_from_slice(&32767i16.to_le_bytes());
        }
        let level = calculate_audio_level(&pcm);
        assert!((level - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_calculate_audio_level_normal_speech_gain() {
        // 正常语音幅度约 2000-4000（原始比率约 0.06 - 0.12）
        let mut pcm = Vec::new();
        for _ in 0..512 {
            pcm.extend_from_slice(&3000i16.to_le_bytes());
        }
        let level = calculate_audio_level(&pcm);
        // 经感知增益后应落在可见的舒适区间 [0.2, 0.9]
        assert!(level > 0.2 && level <= 1.0, "level was {}", level);
    }

    #[test]
    fn test_wav_duration_ms_standard_wav() {
        // 16 kHz 单声道 16 位：500 ms = 8000 采样 = 16000 字节。
        let pcm = vec![0u8; 16000];
        let wav = encode_wav(&pcm, 16000, 1, 16).unwrap();
        assert_eq!(wav_duration_ms(&wav), Some(500));
    }

    #[test]
    fn test_wav_duration_ms_truncates_oversized_data_chunk() {
        let mut wav = encode_wav(&vec![0u8; 3200], 16000, 1, 16).unwrap();
        // 声明的 data 尺寸翻倍，但实际字节只有 100 ms，按实际算。
        let data_len = wav.len() - 44;
        wav[40..44].copy_from_slice(&((data_len as u32) * 2).to_le_bytes());
        assert_eq!(wav_duration_ms(&wav), Some(100));
    }

    #[test]
    fn test_wav_duration_ms_rejects_garbage() {
        assert_eq!(wav_duration_ms(&[]), None);
        assert_eq!(wav_duration_ms(&[0u8; 44]), None);
    }
}
