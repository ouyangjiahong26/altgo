//! 长录音切分：silero-vad 语音边界分组，以及无 VAD 时的 RMS 兜底。
//!
//! 切分规则独立成模块，不依赖识别器就能单测。分段只在 `sherpa` 内部使用。

use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};
use std::path::Path;

/// 触发分段的最短录音时长（毫秒）。更短的录音保持单次推理。
pub(super) const SEGMENT_TRIGGER_MS: u64 = 60_000;

/// 分段时的目标段长（毫秒）。长录音按目标段长在语音边界上切开。
const SEGMENT_TARGET_MS: u64 = 25_000;

/// 单段时长上限（毫秒）。超过上限的连续语音在这里硬切，同时保证每段远低于
/// FireRedASR2 CTC 的 200 秒模型上限。
pub(super) const SEGMENT_LIMIT_MS: u64 = 30_000;

/// 静音判定阈值：帧 RMS 低于此值视为静音。取 -45 dBFS，
/// 高于常见底噪，低于正常语音（-20 dBFS）。
const SILENCE_RMS: f32 = 0.005;

/// 静音检测帧长（毫秒）。
const FRAME_MS: usize = 30;

/// 一个静音点被视为可切的最短连续静音时长（毫秒）。
const MIN_SILENCE_MS: u64 = 500;

/// silero-vad 的语音判定阈值。
const VAD_THRESHOLD: f32 = 0.5;

/// 一段语音结束后连续这么多秒静音才算结束。
const VAD_MIN_SILENCE_SECONDS: f32 = 0.25;

/// 短于这个时长的语音不输出，用来滤掉瞬时噪声。
const VAD_MIN_SPEECH_SECONDS: f32 = 0.25;

/// silero-vad 的窗口长度（采样点），16 kHz 下的标准取值。
const VAD_WINDOW_SIZE: i32 = 512;

/// VAD 内部环形缓冲的时长（秒）。要装得下最长语音段与切分前的预卷。
const VAD_BUFFER_SECONDS: f32 = 60.0;

/// 送入 VAD 的分块长度（采样点）。分块处理并及时取走语音段，长录音的内存不随
/// 输入长度增长。
pub(super) const VAD_CHUNK_SAMPLES: usize = 16_000;

/// 随安装包分发的 silero-vad 模型 SHA-256。
///
/// sherpa-onnx 只在模型文件不存在时返回错误。文件存在但内容损坏时，ONNX Runtime
/// 抛出的 C++ 异常穿过 FFI 边界会直接终止进程，所以加载前自己核对文件完整。
const VAD_MODEL_SHA256: &str = "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6";

/// 音频是否需要分段：超过触发阈值就切，两个本地引擎一致。
pub(super) fn needs_segmentation(duration_ms: u64) -> bool {
    duration_ms > SEGMENT_TRIGGER_MS
}

/// 音频切分点（采样点下标）。`start..end` 为一个切片。
#[derive(Debug, PartialEq)]
pub(super) struct SegmentRange {
    pub(super) start: usize,
    pub(super) end: usize,
}

/// VAD 检出的语音区间（采样点下标，左闭右开）。
pub(super) type SpeechRange = (usize, usize);

/// 模型文件存在且与随包发布的哈希一致。
pub(super) fn vad_model_ready(path: &Path) -> bool {
    crate::download::file_sha256(path).is_ok_and(|sha256| sha256 == VAD_MODEL_SHA256)
}

/// silero-vad 的识别配置。采样率必须与录音一致。
pub(super) fn vad_config(model: &Path) -> VadModelConfig {
    VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(model.to_string_lossy().into_owned()),
            threshold: VAD_THRESHOLD,
            min_silence_duration: VAD_MIN_SILENCE_SECONDS,
            min_speech_duration: VAD_MIN_SPEECH_SECONDS,
            window_size: VAD_WINDOW_SIZE,
            max_speech_duration: SEGMENT_LIMIT_MS as f32 / 1000.0,
        },
        sample_rate: crate::recorder::SAMPLE_RATE as i32,
        num_threads: 1,
        ..Default::default()
    }
}

/// 用给定模型创建 VAD 检测器，模型无法加载时返回 `None`。
pub(super) fn create_detector(model: &Path) -> Option<VoiceActivityDetector> {
    VoiceActivityDetector::create(&vad_config(model), VAD_BUFFER_SECONDS)
}

/// 按静音感知把音频切成不超过 `limit_ms` 的段，相邻段首尾相接、互不重叠。
///
/// 流程：以 30 ms 为帧计算 RMS，找出连续超过 500 ms 的静音段，在目标段长
/// 附近选最优静音点切分。切点是静音段中点，两侧都不含对方的内容。找不到
/// 静音点就在目标段长处硬切。
pub(super) fn split_on_silence(
    samples: &[f32],
    sample_rate: u32,
    limit_ms: u64,
) -> Vec<SegmentRange> {
    let frame_size = sample_rate as usize * FRAME_MS / 1000;
    let total_ms = samples.len() as u64 * 1000 / sample_rate as u64;
    if total_ms <= limit_ms {
        return vec![SegmentRange {
            start: 0,
            end: samples.len(),
        }];
    }

    // 计算每帧 RMS。
    let frames: Vec<f32> = samples
        .chunks(frame_size)
        .map(|chunk| {
            let sum: f32 = chunk.iter().map(|s| s * s).sum();
            (sum / chunk.len().max(1) as f32).sqrt()
        })
        .collect();

    // 找出连续静音段（frame 下标区间）。
    let silence_frames: Vec<(usize, usize)> = {
        let mut ranges = Vec::new();
        let mut start = None;
        for (i, &rms) in frames.iter().enumerate() {
            if rms < SILENCE_RMS {
                start.get_or_insert(i);
            } else if let Some(s) = start.take() {
                let dur_ms = (i - s) as u64 * FRAME_MS as u64;
                if dur_ms >= MIN_SILENCE_MS {
                    ranges.push((s, i));
                }
            }
        }
        if let Some(s) = start {
            let dur_ms = (frames.len() - s) as u64 * FRAME_MS as u64;
            if dur_ms >= MIN_SILENCE_MS {
                ranges.push((s, frames.len()));
            }
        }
        ranges
    };

    // 贪心切分：每次在目标段长附近找最近的静音点。
    let target_samples = sample_rate as usize * SEGMENT_TARGET_MS as usize / 1000;
    let limit_samples = sample_rate as usize * limit_ms as usize / 1000;

    let mut segments = Vec::new();
    let mut pos = 0usize;
    while pos < samples.len() {
        let remaining = samples.len() - pos;
        if remaining <= limit_samples {
            segments.push(SegmentRange {
                start: pos,
                end: samples.len(),
            });
            break;
        }

        // 理想切点 = pos + target_samples。在 limit_samples 范围内找最近的静音段中点。
        let ideal_end = pos + target_samples;
        let max_end = pos + limit_samples;

        let best_cut = silence_frames
            .iter()
            .filter_map(|&(s, e)| {
                let mid_frame = (s + e) / 2;
                let mid_sample = mid_frame * frame_size;
                if mid_sample > pos + frame_size && mid_sample <= max_end {
                    Some(mid_sample)
                } else {
                    None
                }
            })
            .min_by_key(|&cut| cut.abs_diff(ideal_end));

        let cut = best_cut.unwrap_or(ideal_end.min(max_end));
        // 两段在 cut 处首尾相接。留 padding 会让同一段音频进入两次推理，
        // 硬切时直接产出重复文字。
        segments.push(SegmentRange {
            start: pos,
            end: cut,
        });
        pos = cut.max(pos + 1);
    }

    segments
}

/// 取走检测器里已产生的语音段，换算成采样区间。
///
/// `SpeechSegment` 只是队列元素的一份拷贝，读完后先释放再 `pop`，避免 pop 之后
/// 还持有同一队列元素的指针。
pub(super) fn collect_vad_segments(
    detector: &VoiceActivityDetector,
    total: usize,
    ranges: &mut Vec<SpeechRange>,
) {
    loop {
        let Some(segment) = detector.front() else {
            break;
        };
        let start = segment.start().max(0) as usize;
        let end = start.saturating_add(segment.n().max(0) as usize).min(total);
        drop(segment);
        detector.pop();
        if end > start {
            ranges.push((start, end));
        }
    }
}

/// 把 VAD 语音段分组为切分区间。
///
/// 每个区间取一组语音段的起点到终点，组间静音不送进识别器，因此相邻区间之间
/// 可能留有未使用的静音，但绝不重叠，也不会切进 VAD 判定的语音内部。每组合并
/// 到超过目标段长或再加下一段会超过上限就收束。单段语音本身超过上限时按上限
/// 硬切，这是唯一会切进语音的情况。
pub(super) fn split_on_speech_ranges(
    total: usize,
    sample_rate: u32,
    speech: &[SpeechRange],
) -> Vec<SegmentRange> {
    let mut spans: Vec<SpeechRange> = speech
        .iter()
        .map(|&(start, end)| (start.min(total), end.min(total)))
        .filter(|(start, end)| end > start)
        .collect();
    if spans.is_empty() {
        return Vec::new();
    }
    spans.sort_unstable();

    let target = sample_rate as usize * SEGMENT_TARGET_MS as usize / 1000;
    let limit = sample_rate as usize * SEGMENT_LIMIT_MS as usize / 1000;

    let mut bounded = Vec::with_capacity(spans.len());
    for (start, end) in spans {
        let mut pos = start;
        while end - pos > limit {
            bounded.push((pos, pos + limit));
            pos += limit;
        }
        bounded.push((pos, end));
    }

    let mut ranges = Vec::new();
    let mut start = bounded[0].0;
    let mut end = bounded[0].1;
    for &(next_start, next_end) in &bounded[1..] {
        if next_end - start > limit || end - start >= target {
            ranges.push(SegmentRange { start, end });
            start = next_start;
        }
        end = next_end;
    }
    ranges.push(SegmentRange { start, end });
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_needs_segmentation_over_trigger() {
        // 60 秒整不分段，超过就分段，两个引擎一致。
        assert!(!needs_segmentation(5_000));
        assert!(!needs_segmentation(60_000));
        assert!(needs_segmentation(60_001));
        assert!(needs_segmentation(600_000));
    }

    #[test]
    fn test_vad_config_uses_recorder_sample_rate() {
        let config = vad_config(Path::new("silero_vad.onnx"));
        assert_eq!(config.sample_rate, crate::recorder::SAMPLE_RATE as i32);
        assert_eq!(config.silero_vad.max_speech_duration, 30.0);
        assert_eq!(config.silero_vad.window_size, 512);
        assert_eq!(config.silero_vad.model.as_deref(), Some("silero_vad.onnx"));
    }

    #[test]
    fn test_vad_model_ready_checks_sha256() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!vad_model_ready(&dir.path().join("missing.onnx")));

        let corrupt = dir.path().join("corrupt.onnx");
        std::fs::write(&corrupt, b"not a vad model").unwrap();
        assert!(!vad_model_ready(&corrupt));

        let bundled =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../resources/vad/silero_vad.onnx");
        assert!(
            vad_model_ready(&bundled),
            "仓库应包含合法的 silero-vad 模型"
        );
    }

    #[test]
    fn test_split_on_silence_short_audio_returns_single() {
        let samples = vec![0.5f32; 16000 * 5]; // 5 秒
        let segs = split_on_silence(&samples, 16000, SEGMENT_LIMIT_MS);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start, 0);
        assert_eq!(segs[0].end, samples.len());
    }

    #[test]
    fn test_split_on_silence_long_audio_with_silence_splits() {
        // 构造：30 秒语音 + 2 秒静音交替，共 158 秒。
        let sr = 16000usize;
        let speech = vec![0.5f32; sr * 30];
        let silence = vec![0.0001f32; sr * 2];
        let mut samples = Vec::new();
        for _ in 0..4 {
            samples.extend_from_slice(&speech);
            samples.extend_from_slice(&silence);
        }
        samples.extend_from_slice(&speech);

        let segs = split_on_silence(&samples, sr as u32, SEGMENT_LIMIT_MS);
        assert!(segs.len() >= 2, "长音频应切开，实际 {} 段", segs.len());

        // 每段不超过上限。
        let limit_samples = sr * SEGMENT_LIMIT_MS as usize / 1000;
        for (i, seg) in segs.iter().enumerate() {
            assert!(
                seg.end - seg.start <= limit_samples,
                "段 {} 长度 {} 采样超过上限 {}",
                i,
                seg.end - seg.start,
                limit_samples
            );
        }
        // 所有段覆盖完整音频，且相邻段首尾相接、互不重叠：重叠会把同一段
        // 语音送进两次推理，硬切时直接产出重复文字。
        assert_eq!(segs.first().unwrap().start, 0);
        assert_eq!(segs.last().unwrap().end, samples.len());
        for pair in segs.windows(2) {
            assert_eq!(
                pair[0].end, pair[1].start,
                "相邻段重叠或留缝：{:?} 与 {:?}",
                pair[0], pair[1]
            );
        }
    }

    #[test]
    fn test_split_on_silence_no_silence_forces_hard_cut() {
        // 250 秒连续非静音，找不到静音点，硬切。
        let samples = vec![0.5f32; 16000 * 250];
        let segs = split_on_silence(&samples, 16000, SEGMENT_LIMIT_MS);
        assert!(segs.len() >= 2, "连续语音应硬切，实际 {} 段", segs.len());
        let limit_samples = 16000 * SEGMENT_LIMIT_MS as usize / 1000;
        for seg in &segs {
            assert!(seg.end - seg.start <= limit_samples);
        }
    }

    /// 构造每段 `speech_s` 秒、间隔 `gap_s` 秒的 VAD 语音区间。
    fn speech_ranges(count: usize, speech_s: usize, gap_s: usize, sr: usize) -> Vec<SpeechRange> {
        (0..count)
            .map(|i| {
                let start = i * (speech_s + gap_s) * sr;
                (start, start + speech_s * sr)
            })
            .collect()
    }

    fn median_segment_ms(ranges: &[SegmentRange], sr: usize) -> u64 {
        let mut lengths: Vec<u64> = ranges
            .iter()
            .map(|range| (range.end - range.start) as u64 * 1000 / sr as u64)
            .collect();
        lengths.sort_unstable();
        lengths[lengths.len() / 2]
    }

    #[test]
    fn test_split_on_speech_ranges_empty() {
        assert!(split_on_speech_ranges(16000 * 10, 16000, &[]).is_empty());
    }

    #[test]
    fn test_split_on_speech_ranges_single_span() {
        let sr = 16000usize;
        let ranges = split_on_speech_ranges(sr * 10, sr as u32, &[(0, sr * 5)]);
        assert_eq!(
            ranges,
            vec![SegmentRange {
                start: 0,
                end: sr * 5
            }]
        );
    }

    #[test]
    fn test_split_on_speech_ranges_targets_around_25s() {
        // 每段语音 5 秒、间隔 1 秒，共 6 分钟：段长应在目标附近且不超过上限。
        let sr = 16000usize;
        let spans = speech_ranges(60, 5, 1, sr);
        let ranges = split_on_speech_ranges(60 * 6 * sr, sr as u32, &spans);

        assert!(ranges.len() >= 8);
        let limit = sr * SEGMENT_LIMIT_MS as usize / 1000;
        for range in &ranges {
            assert!(range.end > range.start);
            assert!(range.end - range.start <= limit);
        }
        for pair in ranges.windows(2) {
            assert!(pair[0].end <= pair[1].start, "区间重叠：{:?}", pair);
        }
        assert_eq!(ranges.first().unwrap().start, spans.first().unwrap().0);
        assert_eq!(ranges.last().unwrap().end, spans.last().unwrap().1);
        let median = median_segment_ms(&ranges, sr);
        assert!(
            (25_000..=30_000).contains(&median),
            "段长中位数 {} 毫秒不在 25 到 30 秒之间",
            median
        );
    }

    #[test]
    fn test_split_on_speech_ranges_cuts_only_between_spans() {
        let sr = 16000usize;
        let spans = speech_ranges(10, 4, 2, sr);
        let ranges = split_on_speech_ranges(10 * 6 * sr, sr as u32, &spans);

        for range in &ranges {
            // 每个区间的起止都落在某段语音的边界上，说明没有切进语音内部。
            assert!(spans.iter().any(|span| span.0 == range.start));
            assert!(spans.iter().any(|span| span.1 == range.end));
        }
    }

    #[test]
    fn test_split_on_speech_ranges_long_span_hard_cut() {
        // 单段 70 秒连续语音：只能在上限处硬切。
        let sr = 16000usize;
        let ranges = split_on_speech_ranges(sr * 70, sr as u32, &[(0, sr * 70)]);
        let limit = sr * SEGMENT_LIMIT_MS as usize / 1000;
        assert_eq!(ranges.len(), 3);
        for range in &ranges {
            assert!(range.end - range.start <= limit);
        }
        assert_eq!(ranges.last().unwrap().end, sr * 70);
    }
}
