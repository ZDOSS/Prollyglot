use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{VisualFrame, VisualRect};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrObservation {
    pub text: String,
    pub confidence: f32,
    pub language: Option<String>,
    pub script: Option<String>,
    pub bounds: VisualRect,
}

impl OcrObservation {
    pub fn normalized_text(&self) -> String {
        self.text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    pub fn is_usable(&self, minimum_confidence: f32) -> bool {
        !self.normalized_text().is_empty()
            && self.confidence.is_finite()
            && self.confidence >= minimum_confidence
            && self.bounds.is_valid()
    }
}

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("OCR was cancelled")]
    Cancelled,
    #[error("OCR is not available: {0}")]
    Unavailable(String),
    #[error("OCR model is not installed: {0}")]
    ModelMissing(String),
    #[error("OCR inference failed: {0}")]
    Inference(String),
}

pub trait OcrEngine: Send {
    fn recognize(&mut self, frame: &VisualFrame) -> Result<Vec<OcrObservation>, OcrError>;
}
