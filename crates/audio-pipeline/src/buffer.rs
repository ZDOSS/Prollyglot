use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferPushReport {
    pub dropped_samples: usize,
    pub total_dropped_samples: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BufferedAudioChunk {
    pub start_micros: u64,
    pub end_micros: u64,
    pub samples: Vec<f32>,
}

/// A low-latency bounded buffer. When a producer outruns inference, the oldest
/// audio is discarded so captions recover near live playback instead of
/// accumulating unbounded delay.
pub struct BoundedSampleBuffer {
    samples: VecDeque<f32>,
    sample_rate: u32,
    capacity: usize,
    start_micros: Option<u64>,
    total_dropped_samples: u64,
}

impl BoundedSampleBuffer {
    pub fn new(sample_rate: u32, capacity: usize) -> Self {
        assert!(sample_rate > 0, "sample rate must be non-zero");
        assert!(capacity > 0, "buffer capacity must be non-zero");
        Self {
            samples: VecDeque::with_capacity(capacity),
            sample_rate,
            capacity,
            start_micros: None,
            total_dropped_samples: 0,
        }
    }

    pub fn push(&mut self, start_micros: u64, samples: &[f32]) -> BufferPushReport {
        if samples.is_empty() {
            return BufferPushReport {
                total_dropped_samples: self.total_dropped_samples,
                ..BufferPushReport::default()
            };
        }

        let dropped_samples = if samples.len() >= self.capacity {
            let dropped = self.samples.len() + samples.len() - self.capacity;
            let incoming_prefix = samples.len() - self.capacity;
            self.samples.clear();
            self.samples.extend(&samples[incoming_prefix..]);
            self.start_micros = Some(
                start_micros.saturating_add(duration_micros(incoming_prefix, self.sample_rate)),
            );
            dropped
        } else {
            if self.samples.is_empty() {
                self.start_micros = Some(start_micros);
            }
            let overflow = self
                .samples
                .len()
                .saturating_add(samples.len())
                .saturating_sub(self.capacity);
            if overflow > 0 {
                self.samples.drain(..overflow);
                self.start_micros = self
                    .start_micros
                    .map(|start| start.saturating_add(duration_micros(overflow, self.sample_rate)));
            }
            self.samples.extend(samples);
            overflow
        };

        self.total_dropped_samples = self
            .total_dropped_samples
            .saturating_add(dropped_samples as u64);
        BufferPushReport {
            dropped_samples,
            total_dropped_samples: self.total_dropped_samples,
        }
    }

    pub fn pop_exact(&mut self, sample_count: usize) -> Option<BufferedAudioChunk> {
        if sample_count == 0 || self.samples.len() < sample_count {
            return None;
        }
        let start_micros = self.start_micros.unwrap_or_default();
        let end_micros =
            start_micros.saturating_add(duration_micros(sample_count, self.sample_rate));
        let samples = self.samples.drain(..sample_count).collect();
        self.start_micros = if self.samples.is_empty() {
            None
        } else {
            Some(end_micros)
        };
        Some(BufferedAudioChunk {
            start_micros,
            end_micros,
            samples,
        })
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        self.start_micros = None;
    }

    pub fn available_samples(&self) -> usize {
        self.samples.len()
    }

    pub fn total_dropped_samples(&self) -> u64 {
        self.total_dropped_samples
    }
}

fn duration_micros(sample_count: usize, sample_rate: u32) -> u64 {
    (sample_count as u64).saturating_mul(1_000_000) / u64::from(sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_returns_fixed_chunks_with_timestamps() {
        let mut buffer = BoundedSampleBuffer::new(16_000, 3_200);
        buffer.push(500_000, &vec![0.5; 2_000]);
        let chunk = buffer.pop_exact(1_600).expect("one chunk");

        assert_eq!(chunk.start_micros, 500_000);
        assert_eq!(chunk.end_micros, 600_000);
        assert_eq!(chunk.samples.len(), 1_600);
        assert_eq!(buffer.available_samples(), 400);
    }

    #[test]
    fn overflow_drops_oldest_audio_and_advances_time() {
        let mut buffer = BoundedSampleBuffer::new(1_000, 4);
        buffer.push(10_000, &[1.0, 2.0, 3.0]);
        let report = buffer.push(13_000, &[4.0, 5.0, 6.0]);
        let chunk = buffer.pop_exact(4).expect("full buffer");

        assert_eq!(report.dropped_samples, 2);
        assert_eq!(report.total_dropped_samples, 2);
        assert_eq!(chunk.start_micros, 12_000);
        assert_eq!(chunk.samples, vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn oversized_input_keeps_only_its_newest_samples() {
        let mut buffer = BoundedSampleBuffer::new(1_000, 3);
        buffer.push(0, &[9.0]);
        let report = buffer.push(1_000, &[1.0, 2.0, 3.0, 4.0, 5.0]);
        let chunk = buffer.pop_exact(3).expect("full buffer");

        assert_eq!(report.dropped_samples, 3);
        assert_eq!(chunk.start_micros, 3_000);
        assert_eq!(chunk.samples, vec![3.0, 4.0, 5.0]);
    }
}
