//! Shared domain contracts for Prollyglot.
//!
//! Platform capture objects and UI-specific payloads do not belong in this crate.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable identifier supplied by a platform capture backend.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackDevice {
    pub id: SourceId,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationSource {
    pub id: SourceId,
    pub name: String,
    pub process_id: u32,
    /// Playback devices on which an active session for this process was observed.
    pub device_ids: Vec<SourceId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub playback_devices: Vec<PlaybackDevice>,
    pub applications: Vec<ApplicationSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CaptureSelection {
    SystemDefault,
    SystemOutput { device_id: SourceId },
    Application { process_id: u32 },
}

impl CaptureSelection {
    pub fn source_id(&self) -> SourceId {
        match self {
            Self::SystemDefault => SourceId::new("default-output"),
            Self::SystemOutput { device_id } => device_id.clone(),
            Self::Application { process_id } => SourceId::new(format!("process:{process_id}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SampleFormat {
    F32,
    I16,
    I24,
    I32,
}

impl SampleFormat {
    pub const fn bytes_per_sample(self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::I16 => 2,
            Self::I24 => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: SampleFormat,
}

impl NativeAudioFormat {
    pub fn validate(self) -> Result<Self, CaptureError> {
        if self.sample_rate == 0 {
            return Err(CaptureError::InvalidFormat(
                "sample rate must be non-zero".into(),
            ));
        }
        if self.channels == 0 {
            return Err(CaptureError::InvalidFormat(
                "channel count must be non-zero".into(),
            ));
        }
        Ok(self)
    }

    pub const fn bytes_per_frame(self) -> usize {
        self.channels as usize * self.sample_format.bytes_per_sample()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFrame {
    pub sequence: u64,
    pub source_id: SourceId,
    pub captured_at_micros: u64,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
    pub peak: f32,
    pub discontinuity: bool,
}

impl AudioFrame {
    pub fn duration_micros(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        (self.samples.len() as u64 * 1_000_000) / u64::from(self.sample_rate)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureState {
    Starting,
    Capturing,
    Waiting,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Clone, Debug)]
pub enum CaptureEvent {
    State(CaptureState),
    Frame(AudioFrame),
    Warning(String),
    FramesDropped { total: u64 },
    Error(String),
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("audio capture is not supported on this platform")]
    UnsupportedPlatform,
    #[error("the selected source is unavailable: {0}")]
    SourceUnavailable(String),
    #[error("invalid native audio format: {0}")]
    InvalidFormat(String),
    #[error("the capture stream is already running")]
    AlreadyRunning,
    #[error("the capture stream is not running")]
    NotRunning,
    #[error("platform audio operation failed: {context} ({code})")]
    Platform { context: String, code: String },
    #[error("capture worker stopped unexpectedly: {0}")]
    Worker(String),
}

pub trait CaptureSession: Send {
    fn selection(&self) -> &CaptureSelection;
    fn stop(&mut self) -> Result<(), CaptureError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_rejects_zero_dimensions() {
        let invalid_rate = NativeAudioFormat {
            sample_rate: 0,
            channels: 2,
            sample_format: SampleFormat::F32,
        };
        assert!(invalid_rate.validate().is_err());

        let invalid_channels = NativeAudioFormat {
            sample_rate: 48_000,
            channels: 0,
            sample_format: SampleFormat::F32,
        };
        assert!(invalid_channels.validate().is_err());
    }

    #[test]
    fn frame_duration_uses_mono_sample_count() {
        let frame = AudioFrame {
            sequence: 1,
            source_id: SourceId::new("test"),
            captured_at_micros: 0,
            sample_rate: 16_000,
            samples: vec![0.0; 8_000],
            peak: 0.0,
            discontinuity: false,
        };

        assert_eq!(frame.duration_micros(), 500_000);
    }

    #[test]
    fn default_output_has_a_stable_source_identifier() {
        assert_eq!(
            CaptureSelection::SystemDefault.source_id(),
            SourceId::new("default-output")
        );
    }

    #[test]
    fn default_output_matches_the_desktop_ipc_shape() {
        let selection: CaptureSelection =
            serde_json::from_str(r#"{"kind":"systemDefault"}"#).expect("valid selection");

        assert_eq!(selection, CaptureSelection::SystemDefault);
    }
}
