//! Desktop capture adapter. Portal pixel dimensions are useful to OCR, but
//! never authorize positioning a window in physical desktop coordinates.

use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError};
use prollyglot_application_runtime::{CancellationToken, VisualCaptureSelection};
use prollyglot_visual_pipeline::VisualFrame;

#[derive(Clone, Debug)]
pub struct PickedVisualSource {
    pub label: String,
    /// Physical coordinates on Windows; portal geometry is stored separately.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[cfg(target_os = "linux")]
    pub portal: Option<crate::visual_geometry::PortalGeometry>,
}

pub enum VisualCaptureEvent {
    Started(PickedVisualSource),
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    Frame {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        replaced_frames: u64,
    },
    SourceClosed,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Failed(String),
}

#[derive(Clone)]
pub enum CaptureEvents {
    #[cfg(not(target_os = "linux"))]
    Windows(Receiver<prollyglot_visual_windows::VisualCaptureEvent>),
    #[cfg(target_os = "linux")]
    Portal(Receiver<VisualCaptureEvent>),
}

impl CaptureEvents {
    pub fn recv_timeout(&self, timeout: Duration) -> Result<VisualCaptureEvent, RecvTimeoutError> {
        match self {
            #[cfg(not(target_os = "linux"))]
            Self::Windows(events) => {
                use prollyglot_visual_windows::VisualCaptureEvent as Event;
                Ok(match events.recv_timeout(timeout)? {
                    Event::Started(source) => VisualCaptureEvent::Started(source.into()),
                    Event::Frame {
                        x,
                        y,
                        width,
                        height,
                        replaced_frames,
                        ..
                    } => VisualCaptureEvent::Frame {
                        x,
                        y,
                        width,
                        height,
                        replaced_frames,
                    },
                    Event::SourceClosed => VisualCaptureEvent::SourceClosed,
                })
            }
            #[cfg(target_os = "linux")]
            Self::Portal(events) => events.recv_timeout(timeout),
        }
    }
}

pub struct StartedVisualCapture {
    pub source: PickedVisualSource,
    pub frames: Receiver<VisualFrame>,
    pub events: CaptureEvents,
    #[cfg(not(target_os = "linux"))]
    session: prollyglot_visual_windows::StartedVisualCapture,
    #[cfg(target_os = "linux")]
    session: crate::visual_portal::PortalCapture,
}

impl StartedVisualCapture {
    pub fn stop(&mut self) -> Result<(), String> {
        self.session.stop().map_err(|error| error.to_string())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    Cancelled,
    Failed(String),
}

pub fn validate_selection(selection: &VisualCaptureSelection) -> Result<(), String> {
    let portal = matches!(
        selection,
        VisualCaptureSelection::PortalWindow
            | VisualCaptureSelection::PortalDisplay
            | VisualCaptureSelection::PortalRegion
    );
    if portal == cfg!(target_os = "linux") {
        Ok(())
    } else {
        Err(if cfg!(target_os = "linux") {
            "Choose a window, monitor, or region using the desktop picker."
        } else {
            "Choose an available Windows capture source."
        }
        .into())
    }
}

/// Called on a blocking startup worker, never the command/UI thread. Stop is
/// observed while the portal picker waits for its human response.
pub fn start_capture(
    app: tauri::AppHandle,
    selection: VisualCaptureSelection,
    cancellation: CancellationToken,
) -> Result<StartedVisualCapture, StartError> {
    validate_selection(&selection).map_err(StartError::Failed)?;
    if cancellation.is_cancelled() {
        return Err(StartError::Cancelled);
    }
    #[cfg(target_os = "linux")]
    {
        let (session, source, frames, events) =
            crate::visual_portal::start_capture(app, selection, cancellation)?;
        Ok(StartedVisualCapture {
            session,
            source,
            frames,
            events: CaptureEvents::Portal(events),
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        use prollyglot_visual_windows::VisualCaptureSelection as Selection;
        let selection = match selection {
            VisualCaptureSelection::ApplicationWindow { source_id } => {
                Selection::ApplicationWindow { source_id }
            }
            VisualCaptureSelection::Display { source_id } => Selection::Display { source_id },
            VisualCaptureSelection::Region { display_id, region } => Selection::Region {
                display_id,
                region: prollyglot_visual_pipeline::PixelRect {
                    x: region.x,
                    y: region.y,
                    width: region.width,
                    height: region.height,
                },
            },
            _ => unreachable!("validated Windows selection"),
        };
        let session = prollyglot_visual_windows::start_capture(selection)
            .map_err(|error| StartError::Failed(error.to_string()))?;
        Ok(StartedVisualCapture {
            source: session.source.clone().into(),
            frames: session.frames.clone(),
            events: CaptureEvents::Windows(session.events.clone()),
            session,
        })
    }
}

#[cfg(not(target_os = "linux"))]
impl From<prollyglot_visual_windows::PickedVisualSource> for PickedVisualSource {
    fn from(source: prollyglot_visual_windows::PickedVisualSource) -> Self {
        Self {
            label: source.label,
            x: source.x,
            y: source.y,
            width: source.width,
            height: source.height,
        }
    }
}
