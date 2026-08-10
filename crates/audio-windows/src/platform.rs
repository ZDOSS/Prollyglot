use crossbeam_channel::Sender;
use prollyglot_core::{
    CaptureError, CaptureEvent, CaptureSelection, CaptureSession, SourceSnapshot,
};

/// The native implementation is filled in behind this boundary during the
/// Windows-capture portion of Milestone 1. Keeping this module explicit makes a
/// missing backend an honest runtime error instead of silently using microphone
/// input or a fake source.
pub(crate) fn source_snapshot() -> Result<SourceSnapshot, CaptureError> {
    Err(CaptureError::Worker(
        "Windows source enumeration is not connected yet".into(),
    ))
}

pub(crate) fn start_capture(
    _selection: CaptureSelection,
    _events: Sender<CaptureEvent>,
) -> Result<Box<dyn CaptureSession>, CaptureError> {
    Err(CaptureError::Worker(
        "Windows loopback capture is not connected yet".into(),
    ))
}
