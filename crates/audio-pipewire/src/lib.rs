//! Native PipeWire output and application capture for the experimental Ubuntu port.
//! No microphone fallback, audio-routing changes, subprocesses, or recordings.

use crossbeam_channel::Sender;
use prollyglot_core::{
    AudioCaptureBackend, AudioCaptureCapabilities, CaptureError, CaptureEvent, CaptureSelection,
    CaptureSession, ResolvedCaptureSelection, SourceSnapshot,
};

#[cfg(target_os = "linux")]
mod application;
#[cfg(target_os = "linux")]
mod graph;
#[cfg(target_os = "linux")]
mod identity;
#[cfg(target_os = "linux")]
mod platform;
#[cfg(target_os = "linux")]
mod publish;

#[derive(Clone, Copy, Debug, Default)]
pub struct PipeWireAudioCaptureBackend;

impl PipeWireAudioCaptureBackend {
    pub const fn new() -> Self {
        Self
    }
}

impl AudioCaptureBackend for PipeWireAudioCaptureBackend {
    fn capabilities(&self) -> AudioCaptureCapabilities {
        AudioCaptureCapabilities {
            backend: "pipewire".into(),
            available: cfg!(target_os = "linux"),
            system_default: cfg!(target_os = "linux"),
            playback_devices: cfg!(target_os = "linux"),
            applications: cfg!(target_os = "linux"),
            application_restart_recovery: cfg!(target_os = "linux"),
        }
    }

    fn source_snapshot(&self) -> Result<SourceSnapshot, CaptureError> {
        #[cfg(target_os = "linux")]
        {
            platform::source_snapshot()
        }
        #[cfg(not(target_os = "linux"))]
        Err(CaptureError::UnsupportedPlatform)
    }

    fn resolve_selection(
        &self,
        selection: &CaptureSelection,
    ) -> Result<ResolvedCaptureSelection, CaptureError> {
        #[cfg(target_os = "linux")]
        {
            platform::resolve_selection(selection)
        }
        #[cfg(not(target_os = "linux"))]
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
        #[cfg(target_os = "linux")]
        {
            platform::start_capture(selection, events)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (selection, events);
            Err(CaptureError::UnsupportedPlatform)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_only_implemented_sources() {
        let capabilities = PipeWireAudioCaptureBackend::new().capabilities();
        assert_eq!(capabilities.available, cfg!(target_os = "linux"));
        assert_eq!(capabilities.applications, cfg!(target_os = "linux"));
        assert_eq!(
            capabilities.application_restart_recovery,
            cfg!(target_os = "linux")
        );
    }
}
