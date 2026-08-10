use prollyglot_core::{AudioFrame, SourceId};
use thiserror::Error;

use crate::{
    BoundedSampleBuffer, ResamplerError, StreamingResampler, VoiceActivity, VoiceActivityConfig,
    vad::EnergyVoiceDetector,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioPipelineConfig {
    pub target_sample_rate: u32,
    pub chunk_samples: usize,
    pub buffer_samples: usize,
    pub voice_activity: VoiceActivityConfig,
}

impl Default for AudioPipelineConfig {
    fn default() -> Self {
        Self {
            target_sample_rate: 16_000,
            chunk_samples: 1_600,
            buffer_samples: 80_000,
            voice_activity: VoiceActivityConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum AudioPipelineError {
    #[error("sample rate and sizes must be non-zero, and the buffer must hold one full chunk")]
    InvalidConfiguration,
    #[error("audio frame sample rate must be non-zero")]
    InvalidInputSampleRate,
    #[error(transparent)]
    Resampler(#[from] ResamplerError),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PipelinePushReport {
    pub dropped_samples: usize,
    pub total_dropped_samples: u64,
    pub stream_reset: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessedAudioChunk {
    pub start_micros: u64,
    pub end_micros: u64,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
    pub voice_activity: VoiceActivity,
    pub rms: f32,
}

pub struct AudioPipeline {
    config: AudioPipelineConfig,
    buffer: BoundedSampleBuffer,
    detector: EnergyVoiceDetector,
    resampler: Option<StreamingResampler>,
    source_id: Option<SourceId>,
    last_sequence: Option<u64>,
}

impl AudioPipeline {
    pub fn new(config: AudioPipelineConfig) -> Result<Self, AudioPipelineError> {
        if config.target_sample_rate == 0
            || config.chunk_samples == 0
            || config.buffer_samples == 0
            || config.buffer_samples < config.chunk_samples
        {
            return Err(AudioPipelineError::InvalidConfiguration);
        }
        Ok(Self {
            buffer: BoundedSampleBuffer::new(config.target_sample_rate, config.buffer_samples),
            detector: EnergyVoiceDetector::new(config.voice_activity),
            config,
            resampler: None,
            source_id: None,
            last_sequence: None,
        })
    }

    pub fn push_frame(
        &mut self,
        frame: AudioFrame,
    ) -> Result<PipelinePushReport, AudioPipelineError> {
        if frame.sample_rate == 0 {
            return Err(AudioPipelineError::InvalidInputSampleRate);
        }

        let source_changed = self
            .source_id
            .as_ref()
            .is_some_and(|current| current != &frame.source_id);
        let sequence_broken = self
            .last_sequence
            .is_some_and(|last| frame.sequence != last.wrapping_add(1));
        let rate_changed = self
            .resampler
            .as_ref()
            .is_some_and(|resampler| resampler.input_rate() != frame.sample_rate);
        let stream_reset = frame.discontinuity || source_changed || sequence_broken || rate_changed;
        if stream_reset {
            self.reset_stream();
        }

        if self.resampler.is_none() {
            self.resampler = Some(StreamingResampler::new(
                frame.sample_rate,
                self.config.target_sample_rate,
            )?);
        }

        let input_duration =
            (frame.samples.len() as u64).saturating_mul(1_000_000) / u64::from(frame.sample_rate);
        let start_micros = frame.captured_at_micros.saturating_sub(input_duration);
        let resampled = self
            .resampler
            .as_mut()
            .expect("resampler initialized")
            .process(&frame.samples, false)?;
        let report = self.buffer.push(start_micros, &resampled);
        self.source_id = Some(frame.source_id);
        self.last_sequence = Some(frame.sequence);

        Ok(PipelinePushReport {
            dropped_samples: report.dropped_samples,
            total_dropped_samples: report.total_dropped_samples,
            stream_reset,
        })
    }

    pub fn next_chunk(&mut self) -> Option<ProcessedAudioChunk> {
        let chunk = self.buffer.pop_exact(self.config.chunk_samples)?;
        let (voice_activity, rms) = self.detector.observe(&chunk.samples);
        Some(ProcessedAudioChunk {
            start_micros: chunk.start_micros,
            end_micros: chunk.end_micros,
            sample_rate: self.config.target_sample_rate,
            samples: chunk.samples,
            voice_activity,
            rms,
        })
    }

    pub fn reset_stream(&mut self) {
        self.buffer.clear();
        self.detector.reset();
        self.resampler = None;
        self.source_id = None;
        self.last_sequence = None;
    }

    pub fn buffered_samples(&self) -> usize {
        self.buffer.available_samples()
    }
}

#[cfg(test)]
mod tests {
    use prollyglot_core::SourceId;

    use super::*;

    fn frame(sequence: u64, samples: Vec<f32>) -> AudioFrame {
        let duration = samples.len() as u64 * 1_000_000 / 48_000;
        AudioFrame {
            sequence,
            source_id: SourceId::new("device"),
            captured_at_micros: (sequence + 1) * duration,
            sample_rate: 48_000,
            peak: samples.iter().copied().map(f32::abs).fold(0.0, f32::max),
            samples,
            discontinuity: false,
        }
    }

    #[test]
    fn produces_fixed_sixteen_kilohertz_chunks() {
        let mut pipeline = AudioPipeline::new(AudioPipelineConfig {
            chunk_samples: 1_600,
            ..AudioPipelineConfig::default()
        })
        .expect("valid config");
        pipeline
            .push_frame(frame(0, vec![0.2; 5_120]))
            .expect("valid frame");
        let chunk = pipeline.next_chunk().expect("one 100 ms chunk");

        assert_eq!(chunk.sample_rate, 16_000);
        assert_eq!(chunk.samples.len(), 1_600);
        assert_eq!(chunk.voice_activity, VoiceActivity::Started);
        assert_eq!(chunk.start_micros, 0);
        assert_eq!(chunk.end_micros, 100_000);
    }

    #[test]
    fn packet_gap_discards_unfinished_audio() {
        let mut pipeline = AudioPipeline::new(AudioPipelineConfig {
            chunk_samples: 1_600,
            ..AudioPipelineConfig::default()
        })
        .expect("valid config");
        pipeline
            .push_frame(frame(0, vec![0.2; 2_400]))
            .expect("valid frame");
        assert!(pipeline.buffered_samples() > 0);

        let report = pipeline
            .push_frame(frame(2, vec![0.2; 5_120]))
            .expect("valid frame");

        assert!(report.stream_reset);
        assert!(pipeline.buffered_samples() >= 1_600);
    }

    #[test]
    fn bounded_pipeline_reports_dropped_audio() {
        let mut pipeline = AudioPipeline::new(AudioPipelineConfig {
            chunk_samples: 800,
            buffer_samples: 1_000,
            ..AudioPipelineConfig::default()
        })
        .expect("valid config");
        let report = pipeline
            .push_frame(frame(0, vec![0.2; 5_120]))
            .expect("valid frame");

        assert!(report.dropped_samples > 0);
        assert_eq!(report.total_dropped_samples, report.dropped_samples as u64);
        assert_eq!(pipeline.buffered_samples(), 1_000);
    }
}
