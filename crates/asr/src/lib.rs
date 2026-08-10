//! Backend-neutral contracts for local streaming speech recognition.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ENGLISH_LANGUAGE_TAG: &str = "en";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InferenceDevice {
    Cpu,
    Gpu,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechEngineInfo {
    pub id: String,
    pub name: String,
    pub languages: Vec<String>,
    pub streaming: bool,
    pub supported_devices: Vec<InferenceDevice>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineState {
    Unloaded,
    Loaded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelLocation {
    pub id: String,
    pub directory: PathBuf,
    /// Resolved model artifacts keyed by backend-defined roles such as
    /// `encoder`, `decoder`, `joiner`, and `tokens`.
    pub artifacts: BTreeMap<String, PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechStreamConfig {
    pub sample_rate: u32,
    pub language: String,
}

impl Default for SpeechStreamConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            language: ENGLISH_LANGUAGE_TAG.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpeechAudio {
    pub start_micros: u64,
    pub end_micros: u64,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechHypothesis {
    pub utterance_id: u64,
    pub text: String,
    pub start_micros: u64,
    pub end_micros: u64,
    pub language: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "result", rename_all = "camelCase")]
pub enum SpeechEvent {
    Partial(SpeechHypothesis),
    Final(SpeechHypothesis),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeechErrorKind {
    MissingModel,
    CorruptModel,
    UnsupportedLanguage,
    UnsupportedDevice,
    InvalidAudio,
    InsufficientMemory,
    BackendUnavailable,
    Internal,
}

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct SpeechError {
    pub kind: SpeechErrorKind,
    pub message: String,
    pub recoverable: bool,
}

impl SpeechError {
    pub fn new(kind: SpeechErrorKind, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            kind,
            message: message.into(),
            recoverable,
        }
    }
}

/// A loaded speech backend. Implementations must not perform inference on an
/// operating-system audio callback thread.
pub trait SpeechEngine: Send {
    fn info(&self) -> &SpeechEngineInfo;
    fn state(&self) -> EngineState;
    fn load_model(&mut self, model: &ModelLocation) -> Result<(), SpeechError>;
    fn unload_model(&mut self) -> Result<(), SpeechError>;
    fn start_stream(
        &self,
        config: SpeechStreamConfig,
    ) -> Result<Box<dyn SpeechStream>, SpeechError>;
}

/// One sequential recognition stream. Final events are append-only; a backend
/// may revise only the current partial hypothesis.
pub trait SpeechStream: Send {
    fn push_audio(&mut self, audio: SpeechAudio) -> Result<Vec<SpeechEvent>, SpeechError>;
    fn end_utterance(&mut self, at_micros: u64) -> Result<Vec<SpeechEvent>, SpeechError>;
    fn finish(&mut self, at_micros: u64) -> Result<Vec<SpeechEvent>, SpeechError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_stream_is_local_english_at_sixteen_kilohertz() {
        assert_eq!(
            SpeechStreamConfig::default(),
            SpeechStreamConfig {
                sample_rate: 16_000,
                language: "en".into(),
            }
        );
    }

    #[test]
    fn structured_errors_survive_ipc_serialization() {
        let error = SpeechError::new(
            SpeechErrorKind::MissingModel,
            "Install the English caption model.",
            true,
        );
        let value = serde_json::to_value(&error).expect("serialize error");

        assert_eq!(value["kind"], "missingModel");
        assert_eq!(value["recoverable"], true);
    }
}
