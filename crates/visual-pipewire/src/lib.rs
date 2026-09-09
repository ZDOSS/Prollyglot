//! Ubuntu screen capture using XDG Desktop Portal and its restricted PipeWire FD.
//!
//! This is a capture backend, not a desktop overlay implementation. The portal's
//! optional logical geometry is deliberately separate from OCR pixel geometry.
//! Nothing is captured until the caller explicitly starts a session and the
//! owner accepts the desktop picker. Pixels remain in the bounded, transient
//! in-process channel; this crate has no recording or serialization API.

#[cfg(target_os = "linux")]
mod dmabuf;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "linux")]
mod video;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

use crossbeam_channel::Receiver;
use prollyglot_visual_pipeline::{PixelRect, VisualFrame};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalSource {
    Monitor,
    Window,
}

#[derive(Clone, Debug)]
pub struct CaptureOptions {
    pub source: PortalSource,
    /// Exported portal parent identifier (`x11:…` or `wayland:…`). Empty means
    /// unparented, as allowed by the portal; callers should supply their window.
    pub parent_window: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamInfo {
    pub source: PortalSource,
    /// Compositor logical coordinates, never physical display pixels. Windows
    /// have no portable global position, even when a backend supplies one.
    pub logical_position: Option<(i32, i32)>,
    pub logical_size: Option<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameGeometry {
    pub raw_width: u32,
    pub raw_height: u32,
    pub crop: PixelRect,
    /// SPA video transform: 0–3 normal/90/180/270° counter-clockwise;
    /// 4–7 horizontal flip followed by the corresponding rotation.
    pub transform: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureEvent {
    Selected(StreamInfo),
    /// Emitted before frames begin and whenever buffer geometry changes.
    FormatChanged(FrameGeometry),
    /// The owner dismissed the picker or the caller stopped during startup.
    Cancelled,
    /// The desktop revoked sharing, the source disappeared, or the session ended.
    Closed,
    Failed(String),
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("portal screen capture is only available on Linux")]
    UnsupportedPlatform,
    #[error("screen capture was cancelled")]
    Cancelled,
    #[error("screen sharing ended")]
    Closed,
    #[error("screen capture failed: {0}")]
    Failed(String),
}

pub struct CaptureSession {
    pub frames: Receiver<VisualFrame>,
    pub events: Receiver<CaptureEvent>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl CaptureSession {
    /// Cancels an open picker as well as an active stream, releases the portal
    /// session, and joins the worker. Repeated calls are harmless.
    pub fn stop(&mut self) -> Result<(), CaptureError> {
        self.stop.store(true, Ordering::Release);
        let result = self.worker.take().map_or(Ok(()), |worker| {
            worker
                .join()
                .map_err(|_| CaptureError::Failed("capture worker panicked".into()))
        });
        // Do not leave an old image queued after sharing has ended.
        while self.frames.try_recv().is_ok() {}
        result
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Starts asynchronously. Selection, geometry and terminal state arrive as
/// events; the caller remains able to Stop while the desktop picker is open.
#[cfg(target_os = "linux")]
pub fn start_capture(options: CaptureOptions) -> Result<CaptureSession, CaptureError> {
    portal::start(options)
}

#[cfg(not(target_os = "linux"))]
pub fn start_capture(_options: CaptureOptions) -> Result<CaptureSession, CaptureError> {
    Err(CaptureError::UnsupportedPlatform)
}
