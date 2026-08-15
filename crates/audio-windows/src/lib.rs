//! Windows playback-device and application audio capture.
//!
//! The public boundary remains available on other hosts so the desktop shell and
//! shared crates can be checked without pretending that WASAPI is available.

use crossbeam_channel::Sender;
use prollyglot_core::{
    AudioCaptureBackend, AudioCaptureCapabilities, CaptureError, CaptureEvent, CaptureSelection,
    CaptureSession, ResolvedCaptureSelection, SourceSnapshot,
};

#[cfg(any(target_os = "windows", test))]
mod identity;

#[cfg(target_os = "windows")]
mod platform;

#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsAudioCaptureBackend;

impl WindowsAudioCaptureBackend {
    pub const fn new() -> Self {
        Self
    }
}

impl AudioCaptureBackend for WindowsAudioCaptureBackend {
    fn capabilities(&self) -> AudioCaptureCapabilities {
        AudioCaptureCapabilities {
            backend: "wasapi".into(),
            available: cfg!(target_os = "windows"),
            system_default: cfg!(target_os = "windows"),
            playback_devices: cfg!(target_os = "windows"),
            applications: cfg!(target_os = "windows"),
            application_restart_recovery: cfg!(target_os = "windows"),
        }
    }

    fn source_snapshot(&self) -> Result<SourceSnapshot, CaptureError> {
        source_snapshot()
    }

    fn resolve_selection(
        &self,
        selection: &CaptureSelection,
    ) -> Result<ResolvedCaptureSelection, CaptureError> {
        #[cfg(target_os = "windows")]
        {
            platform::resolve_selection(selection)
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = selection;
            Err(CaptureError::UnsupportedPlatform)
        }
    }

    fn start_capture(
        &self,
        selection: CaptureSelection,
        events: Sender<CaptureEvent>,
    ) -> Result<Box<dyn CaptureSession>, CaptureError> {
        start_capture(selection, events)
    }
}

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

#[cfg(test)]
mod tests {
    use prollyglot_core::AudioCaptureBackend;

    use super::WindowsAudioCaptureBackend;

    #[test]
    fn capabilities_match_the_compilation_target() {
        let capabilities = WindowsAudioCaptureBackend::new().capabilities();
        assert_eq!(capabilities.backend, "wasapi");
        assert_eq!(capabilities.available, cfg!(target_os = "windows"));
        assert_eq!(capabilities.applications, cfg!(target_os = "windows"));
        assert_eq!(
            capabilities.application_restart_recovery,
            cfg!(target_os = "windows")
        );
    }
}
