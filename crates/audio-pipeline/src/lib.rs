//! Native PCM decoding and normalization shared by capture backends.

mod buffer;
mod pipeline;
mod resampler;
mod vad;

pub use buffer::{BoundedSampleBuffer, BufferPushReport, BufferedAudioChunk};
pub use pipeline::{
    AudioPipeline, AudioPipelineConfig, AudioPipelineError, PipelinePushReport, ProcessedAudioChunk,
};
pub use resampler::{ResamplerError, StreamingResampler};
pub use vad::{EnergyVoiceDetector, VoiceActivity, VoiceActivityConfig};

use std::time::Duration;

use prollyglot_core::{
    AudioFrame, CaptureError, CaptureState, NativeAudioFormat, SampleFormat, SourceId,
};

/// Converts signal activity into stable capture-state transitions without
/// treating an ordinary pause in playback as a capture failure.
pub struct SignalActivity {
    silence_timeout: Duration,
    signal_threshold: f32,
    last_signal_at: Duration,
    waiting: bool,
}

impl SignalActivity {
    pub fn new(silence_timeout: Duration, signal_threshold: f32) -> Self {
        Self {
            silence_timeout,
            signal_threshold: signal_threshold.max(0.0),
            last_signal_at: Duration::ZERO,
            waiting: false,
        }
    }

    pub fn observe(&mut self, elapsed: Duration, peak: f32) -> Option<CaptureState> {
        if peak > self.signal_threshold {
            self.last_signal_at = elapsed;
            if self.waiting {
                self.waiting = false;
                return Some(CaptureState::Capturing);
            }
            return None;
        }
        self.check_for_silence(elapsed)
    }

    pub fn tick(&mut self, elapsed: Duration) -> Option<CaptureState> {
        self.check_for_silence(elapsed)
    }

    pub const fn is_waiting(&self) -> bool {
        self.waiting
    }

    fn check_for_silence(&mut self, elapsed: Duration) -> Option<CaptureState> {
        if !self.waiting && elapsed.saturating_sub(self.last_signal_at) >= self.silence_timeout {
            self.waiting = true;
            Some(CaptureState::Waiting)
        } else {
            None
        }
    }
}

pub fn normalize_interleaved(
    sequence: u64,
    source_id: SourceId,
    captured_at_micros: u64,
    format: NativeAudioFormat,
    bytes: &[u8],
    silent: bool,
    discontinuity: bool,
) -> Result<AudioFrame, CaptureError> {
    let format = format.validate()?;
    let bytes_per_frame = format.bytes_per_frame();
    if !bytes.len().is_multiple_of(bytes_per_frame) {
        return Err(CaptureError::InvalidFormat(format!(
            "{} bytes is not a whole number of {}-byte frames",
            bytes.len(),
            bytes_per_frame
        )));
    }

    let frame_count = bytes.len() / bytes_per_frame;
    let mut samples = vec![0.0_f32; frame_count];
    if !silent {
        for (frame_index, output) in samples.iter_mut().enumerate() {
            let frame_start = frame_index * bytes_per_frame;
            let mut sum = 0.0_f32;
            for channel in 0..format.channels as usize {
                let sample_start = frame_start + channel * format.sample_format.bytes_per_sample();
                sum += decode_sample(format.sample_format, &bytes[sample_start..]);
            }
            let mixed = sum / f32::from(format.channels);
            *output = if mixed.is_finite() {
                mixed.clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }
    }

    let peak = samples
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0_f32, f32::max);
    Ok(AudioFrame {
        sequence,
        source_id,
        captured_at_micros,
        sample_rate: format.sample_rate,
        samples,
        peak,
        discontinuity,
    })
}

fn decode_sample(format: SampleFormat, bytes: &[u8]) -> f32 {
    match format {
        SampleFormat::F32 => f32::from_le_bytes(bytes[..4].try_into().expect("four bytes")),
        SampleFormat::I16 => {
            let sample = i16::from_le_bytes(bytes[..2].try_into().expect("two bytes"));
            f32::from(sample) / 32_768.0
        }
        SampleFormat::I24 => {
            let raw =
                i32::from(bytes[0]) | (i32::from(bytes[1]) << 8) | (i32::from(bytes[2]) << 16);
            let signed = if raw & 0x0080_0000 != 0 {
                raw | !0x00ff_ffff
            } else {
                raw
            };
            signed as f32 / 8_388_608.0
        }
        SampleFormat::I32 => {
            let sample = i32::from_le_bytes(bytes[..4].try_into().expect("four bytes"));
            sample as f32 / 2_147_483_648.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(sample_format: SampleFormat, channels: u16) -> NativeAudioFormat {
        NativeAudioFormat {
            sample_rate: 48_000,
            channels,
            sample_format,
        }
    }

    #[test]
    fn downmixes_stereo_i16_to_mono() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&i16::MAX.to_le_bytes());
        bytes.extend_from_slice(&0_i16.to_le_bytes());
        bytes.extend_from_slice(&i16::MIN.to_le_bytes());
        bytes.extend_from_slice(&0_i16.to_le_bytes());

        let frame = normalize_interleaved(
            4,
            SourceId::new("device"),
            10,
            format(SampleFormat::I16, 2),
            &bytes,
            false,
            false,
        )
        .expect("valid frame");

        assert!((frame.samples[0] - 0.5).abs() < 0.001);
        assert!((frame.samples[1] + 0.5).abs() < 0.001);
        assert!((frame.peak - 0.5).abs() < 0.001);
    }

    #[test]
    fn decodes_signed_i24() {
        let bytes = [0x00, 0x00, 0x40, 0x00, 0x00, 0xc0];
        let frame = normalize_interleaved(
            0,
            SourceId::new("device"),
            0,
            format(SampleFormat::I24, 1),
            &bytes,
            false,
            false,
        )
        .expect("valid frame");

        assert!((frame.samples[0] - 0.5).abs() < 0.000_01);
        assert!((frame.samples[1] + 0.5).abs() < 0.000_01);
    }

    #[test]
    fn silent_flag_ignores_nonzero_buffer() {
        let bytes = f32::MAX.to_le_bytes();
        let frame = normalize_interleaved(
            0,
            SourceId::new("device"),
            0,
            format(SampleFormat::F32, 1),
            &bytes,
            true,
            false,
        )
        .expect("valid frame");

        assert_eq!(frame.samples, vec![0.0]);
        assert_eq!(frame.peak, 0.0);
    }

    #[test]
    fn replaces_non_finite_float_samples_with_silence() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&f32::NAN.to_le_bytes());
        bytes.extend_from_slice(&f32::INFINITY.to_le_bytes());
        let frame = normalize_interleaved(
            0,
            SourceId::new("device"),
            0,
            format(SampleFormat::F32, 1),
            &bytes,
            false,
            false,
        )
        .expect("native float buffers remain usable");

        assert_eq!(frame.samples, vec![0.0, 0.0]);
        assert_eq!(frame.peak, 0.0);
    }

    #[test]
    fn rejects_partial_native_frame() {
        let error = normalize_interleaved(
            0,
            SourceId::new("device"),
            0,
            format(SampleFormat::F32, 2),
            &[0; 7],
            false,
            false,
        )
        .expect_err("partial frame should fail");

        assert!(matches!(error, CaptureError::InvalidFormat(_)));
    }

    #[test]
    fn signal_activity_waits_once_after_sustained_silence() {
        let mut activity = SignalActivity::new(Duration::from_secs(2), 0.000_1);

        assert_eq!(activity.tick(Duration::from_millis(1_999)), None);
        assert_eq!(
            activity.tick(Duration::from_secs(2)),
            Some(CaptureState::Waiting)
        );
        assert_eq!(activity.tick(Duration::from_secs(3)), None);
        assert!(activity.is_waiting());
    }

    #[test]
    fn signal_activity_resumes_and_restarts_silence_window() {
        let mut activity = SignalActivity::new(Duration::from_secs(2), 0.000_1);
        assert_eq!(
            activity.tick(Duration::from_secs(2)),
            Some(CaptureState::Waiting)
        );
        assert_eq!(
            activity.observe(Duration::from_secs(3), 0.25),
            Some(CaptureState::Capturing)
        );
        assert_eq!(activity.tick(Duration::from_millis(4_999)), None);
        assert_eq!(
            activity.tick(Duration::from_secs(5)),
            Some(CaptureState::Waiting)
        );
    }
}
