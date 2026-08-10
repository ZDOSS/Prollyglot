//! Windows playback-device and application audio capture.
//!
//! The public boundary remains available on other hosts so the desktop shell and
//! shared crates can be checked without pretending that WASAPI is available.

use crossbeam_channel::Sender;
use prollyglot_core::{
    CaptureError, CaptureEvent, CaptureSelection, CaptureSession, SourceSnapshot,
};

#[cfg(target_os = "windows")]
mod platform;

/// Enumerate playback endpoints and applications with active audio sessions.
pub fn source_snapshot() -> Result<SourceSnapshot, CaptureError> {
    #[cfg(target_os = "windows")]
    {
        platform::source_snapshot()
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err(CaptureError::UnsupportedPlatform)
    }
}

/// Start one Windows capture worker and publish bounded lifecycle/audio events.
pub fn start_capture(
    selection: CaptureSelection,
    events: Sender<CaptureEvent>,
) -> Result<Box<dyn CaptureSession>, CaptureError> {
    #[cfg(target_os = "windows")]
    {
        platform::start_capture(selection, events)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (selection, events);
        Err(CaptureError::UnsupportedPlatform)
    }
}
