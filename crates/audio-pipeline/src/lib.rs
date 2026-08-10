//! Native PCM decoding and normalization shared by capture backends.

use prollyglot_core::{AudioFrame, CaptureError, NativeAudioFormat, SampleFormat, SourceId};

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
            *output = (sum / f32::from(format.channels)).clamp(-1.0, 1.0);
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
}
