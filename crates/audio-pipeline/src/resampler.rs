use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ResamplerError {
    #[error("input and output sample rates must be non-zero")]
    InvalidSampleRate,
}

/// A chunk-stable linear resampler. One look-ahead sample is retained between
/// calls so splitting the same waveform into different packet sizes produces
/// the same output.
pub struct StreamingResampler {
    input_rate: u32,
    output_rate: u32,
    step: f64,
    buffered: Vec<f32>,
    position: f64,
}

impl StreamingResampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Result<Self, ResamplerError> {
        if input_rate == 0 || output_rate == 0 {
            return Err(ResamplerError::InvalidSampleRate);
        }
        Ok(Self {
            input_rate,
            output_rate,
            step: f64::from(input_rate) / f64::from(output_rate),
            buffered: Vec::new(),
            position: 0.0,
        })
    }

    pub const fn input_rate(&self) -> u32 {
        self.input_rate
    }

    pub const fn output_rate(&self) -> u32 {
        self.output_rate
    }

    pub fn process(&mut self, samples: &[f32], flush: bool) -> Vec<f32> {
        self.buffered.extend_from_slice(samples);
        let mut output = Vec::with_capacity(
            ((samples.len() as f64 * f64::from(self.output_rate) / f64::from(self.input_rate))
                .ceil() as usize)
                .saturating_add(1),
        );

        while self.position < self.buffered.len() as f64 {
            let left = self.position.floor() as usize;
            if !flush && left.saturating_add(1) >= self.buffered.len() {
                break;
            }
            let right = left
                .saturating_add(1)
                .min(self.buffered.len().saturating_sub(1));
            let fraction = (self.position - left as f64) as f32;
            let sample =
                self.buffered[left] + (self.buffered[right] - self.buffered[left]) * fraction;
            output.push(sample);
            self.position += self.step;
        }

        if flush {
            self.reset();
        } else if !self.buffered.is_empty() {
            let consumed = self.position.floor() as usize;
            let drain = consumed.min(self.buffered.len().saturating_sub(1));
            if drain > 0 {
                self.buffered.drain(..drain);
                self.position -= drain as f64;
            }
        }

        output
    }

    pub fn reset(&mut self) {
        self.buffered.clear();
        self.position = 0.0;
    }
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
        let input: Vec<f32> = (0..48_000).map(|index| index as f32).collect();
        let output = resampler.process(&input, true);

        assert_eq!(output.len(), 16_000);
        assert_eq!(output[1], 3.0);
        assert_eq!(output[15_999], 47_997.0);
    }

    #[test]
    fn packet_boundaries_do_not_change_resampled_audio() {
        let input: Vec<f32> = (0..2_003)
            .map(|index| ((index as f32) / 30.0).sin())
            .collect();
        let mut whole = StreamingResampler::new(44_100, 16_000).expect("valid rates");
        let expected = whole.process(&input, true);

        let mut chunked = StreamingResampler::new(44_100, 16_000).expect("valid rates");
        let mut actual = Vec::new();
        for chunk in input.chunks(137) {
            actual.extend(chunked.process(chunk, false));
        }
        actual.extend(chunked.process(&[], true));

        assert_eq!(actual.len(), expected.len());
        for (left, right) in actual.iter().zip(expected) {
            assert!((left - right).abs() < 0.000_01);
        }
    }
}
