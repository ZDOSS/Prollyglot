use rubato::{
    Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters,
    audioadapter_buffers::direct::InterleavedSlice,
};
use thiserror::Error;

const MINIMUM_INPUT_CHUNK_SAMPLES: usize = 512;
const INPUT_CHUNKS_PER_SECOND: u32 = 100;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ResamplerError {
    #[error("input and output sample rates must be non-zero")]
    InvalidSampleRate,
    #[error("could not initialize the band-limited audio resampler: {0}")]
    Construction(String),
    #[error("band-limited audio resampling failed: {0}")]
    Processing(String),
}

/// A packet-boundary-independent, band-limited mono resampler. Windows audio
/// normally arrives at 44.1 or 48 kHz; sinc filtering prevents content above
/// the 16 kHz model rate's Nyquist limit from aliasing into speech frequencies.
pub struct StreamingResampler {
    input_rate: u32,
    output_rate: u32,
    resampler: Option<Async<f32>>,
    buffered: Vec<f32>,
    total_input_samples: u64,
    total_output_samples: u64,
}

impl StreamingResampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Result<Self, ResamplerError> {
        if input_rate == 0 || output_rate == 0 {
            return Err(ResamplerError::InvalidSampleRate);
        }
        let resampler = if input_rate == output_rate {
            None
        } else {
            let chunk_size = usize::try_from(input_rate / INPUT_CHUNKS_PER_SECOND)
                .unwrap_or(MINIMUM_INPUT_CHUNK_SAMPLES)
                .max(MINIMUM_INPUT_CHUNK_SAMPLES);
            Some(
                Async::<f32>::new_sinc(
                    f64::from(output_rate) / f64::from(input_rate),
                    1.0,
                    &SincInterpolationParameters::default(),
                    chunk_size,
                    1,
                    FixedAsync::Input,
                )
                .map_err(|error| ResamplerError::Construction(error.to_string()))?,
            )
        };
        Ok(Self {
            input_rate,
            output_rate,
            resampler,
            buffered: Vec::new(),
            total_input_samples: 0,
            total_output_samples: 0,
        })
    }

    pub const fn input_rate(&self) -> u32 {
        self.input_rate
    }

    pub const fn output_rate(&self) -> u32 {
        self.output_rate
    }

    pub fn process(&mut self, samples: &[f32], flush: bool) -> Result<Vec<f32>, ResamplerError> {
        if self.resampler.is_none() {
            return Ok(samples.to_vec());
        }

        self.total_input_samples = self
            .total_input_samples
            .saturating_add(samples.len() as u64);
        self.buffered.extend_from_slice(samples);
        let mut output = Vec::with_capacity(
            ((samples.len() as f64 * f64::from(self.output_rate) / f64::from(self.input_rate))
                .ceil() as usize)
                .saturating_add(32),
        );

        while self.buffered.len() >= self.next_input_frames() {
            let needed = self.next_input_frames();
            let data = self.process_chunk(needed, None)?;
            self.buffered.drain(..needed);
            self.append_output(data, &mut output, None);
        }

        if flush {
            let expected_total = expected_output_samples(
                self.total_input_samples,
                self.input_rate,
                self.output_rate,
            );
            if !self.buffered.is_empty() {
                let valid = self.buffered.len();
                let needed = self.next_input_frames();
                let data = self.process_chunk(needed, Some(valid))?;
                self.buffered.clear();
                self.append_output(data, &mut output, Some(expected_total));
            }
            while self.total_output_samples < expected_total {
                let needed = self.next_input_frames();
                let data = self.process_chunk(needed, Some(0))?;
                self.append_output(data, &mut output, Some(expected_total));
            }
            self.reset();
        }

        Ok(output)
    }

    pub fn reset(&mut self) {
        if let Some(resampler) = self.resampler.as_mut() {
            resampler.reset();
        }
        self.buffered.clear();
        self.total_input_samples = 0;
        self.total_output_samples = 0;
    }

    fn next_input_frames(&self) -> usize {
        self.resampler
            .as_ref()
            .expect("resampler exists for differing sample rates")
            .input_frames_next()
    }

    fn process_chunk(
        &mut self,
        needed: usize,
        partial_len: Option<usize>,
    ) -> Result<Vec<f32>, ResamplerError> {
        let mut input = vec![0.0_f32; needed];
        let available = partial_len.unwrap_or(needed).min(self.buffered.len());
        input[..available].copy_from_slice(&self.buffered[..available]);
        let adapter = InterleavedSlice::new(&input, 1, needed)
            .map_err(|error| ResamplerError::Processing(error.to_string()))?;
        let indexing = partial_len.map(|length| Indexing::new().partial_len(length));
        self.resampler
            .as_mut()
            .expect("resampler exists for differing sample rates")
            .process(&adapter, indexing.as_ref())
            .map(|output| output.take_data())
            .map_err(|error| ResamplerError::Processing(error.to_string()))
    }

    fn append_output(
        &mut self,
        mut samples: Vec<f32>,
        output: &mut Vec<f32>,
        expected_total: Option<u64>,
    ) {
        if let Some(expected_total) = expected_total {
            let remaining = expected_total.saturating_sub(self.total_output_samples) as usize;
            samples.truncate(remaining);
        }
        self.total_output_samples = self
            .total_output_samples
            .saturating_add(samples.len() as u64);
        output.extend(samples);
    }
}

fn expected_output_samples(input_samples: u64, input_rate: u32, output_rate: u32) -> u64 {
    input_samples
        .saturating_mul(u64::from(output_rate))
        .saturating_add(u64::from(input_rate) - 1)
        / u64::from(input_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_sample_rates() {
        assert!(matches!(
            StreamingResampler::new(0, 16_000),
            Err(ResamplerError::InvalidSampleRate)
        ));
    }

    #[test]
    fn downsampling_one_second_has_the_expected_length() {
        let mut resampler = StreamingResampler::new(48_000, 16_000).expect("valid rates");
        let input = vec![0.25_f32; 48_000];
        let output = resampler.process(&input, true).expect("resample audio");

        assert_eq!(output.len(), 16_000);
        assert!(output.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn packet_boundaries_do_not_change_resampled_audio() {
        let input: Vec<f32> = (0..20_003)
            .map(|index| ((index as f32) / 30.0).sin())
            .collect();
        let mut whole = StreamingResampler::new(44_100, 16_000).expect("valid rates");
        let expected = whole.process(&input, true).expect("whole audio");

        let mut chunked = StreamingResampler::new(44_100, 16_000).expect("valid rates");
        let mut actual = Vec::new();
        for chunk in input.chunks(137) {
            actual.extend(chunked.process(chunk, false).expect("packet audio"));
        }
        actual.extend(chunked.process(&[], true).expect("flush audio"));

        assert_eq!(actual, expected);
    }

    #[test]
    fn downsampling_suppresses_frequencies_above_the_output_nyquist_limit() {
        const INPUT_RATE: u32 = 48_000;
        const OUTPUT_RATE: u32 = 16_000;
        const FREQUENCY: f32 = 12_000.0;
        let input: Vec<f32> = (0..INPUT_RATE)
            .map(|index| {
                (2.0 * std::f32::consts::PI * FREQUENCY * index as f32 / INPUT_RATE as f32).sin()
            })
            .collect();
        let mut resampler = StreamingResampler::new(INPUT_RATE, OUTPUT_RATE).expect("valid rates");
        let output = resampler.process(&input, true).expect("resample audio");
        let settled = &output[1_000..output.len() - 1_000];
        let rms = (settled.iter().map(|sample| sample * sample).sum::<f32>()
            / settled.len() as f32)
            .sqrt();

        assert!(rms < 0.01, "out-of-band RMS was {rms}");
    }

    #[test]
    fn equal_rates_preserve_samples_exactly() {
        let input = vec![-0.25, 0.0, 0.5, 1.0];
        let mut resampler = StreamingResampler::new(16_000, 16_000).expect("valid rates");

        assert_eq!(
            resampler.process(&input, false).expect("same-rate audio"),
            input
        );
    }
}
