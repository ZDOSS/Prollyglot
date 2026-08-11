//! Platform-neutral processing contracts for transient visual text translation.
//!
//! Operating-system capture objects stay in platform crates. Raw pixels are
//! intentionally non-serializable so IPC and diagnostics can carry only
//! geometry, recognized text, state, and aggregate timing/counter data.

mod change;
mod frame;
mod geometry;
mod latest;
mod ocr;
mod pipeline;
mod stabilize;

pub use change::{
    DEFAULT_CAPTURE_FRAME_INTERVAL_MICROS, DEFAULT_LIVE_CAPTURE_FPS, DEFAULT_OCR_INTERVAL_MICROS,
    FrameGate, FrameGateConfig, FrameGateDecision,
};
pub use frame::{PixelFormat, VisualFrame};
pub use geometry::{PixelRect, VisualRect};
pub use latest::{LatestFrameSend, LatestFrameSender, latest_frame_channel};
pub use ocr::{OcrEngine, OcrError, OcrObservation};
pub use pipeline::{VisualPipeline, VisualPipelineStats, VisualProcessOutcome};
pub use stabilize::{StabilizerUpdate, StableTextRegion, TextStabilizer, TextStabilizerConfig};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VisualPipelineError {
    #[error("invalid visual frame: {0}")]
    InvalidFrame(String),
    #[error("the selected crop is outside the captured frame")]
    InvalidCrop,
}
