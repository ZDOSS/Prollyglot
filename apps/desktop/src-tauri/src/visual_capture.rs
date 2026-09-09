//! Desktop capture adapter. Portal pixel dimensions are useful to OCR, but
//! never authorize positioning a window in physical desktop coordinates.

use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError};
use prollyglot_application_runtime::{CancellationToken, VisualCaptureSelection};
use prollyglot_visual_pipeline::VisualFrame;

#[derive(Clone, Debug)]
pub struct PickedVisualSource {
    pub label: String,
    /// Physical coordinates on Windows; unused for the portal reader.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
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
    Portal {
        events: Receiver<prollyglot_visual_pipewire::CaptureEvent>,
        label: String,
    },
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
            Self::Portal { events, label } => {
                use prollyglot_visual_pipewire::CaptureEvent as Event;
                Ok(match events.recv_timeout(timeout)? {
                    Event::FormatChanged(geometry) => {
                        VisualCaptureEvent::Started(portal_geometry(label, geometry))
                    }
                    Event::Selected(_) => return Err(RecvTimeoutError::Timeout),
                    Event::Cancelled | Event::Closed => VisualCaptureEvent::SourceClosed,
                    Event::Failed(message) => VisualCaptureEvent::Failed(message),
                })
            }
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
    session: prollyglot_visual_pipewire::CaptureSession,
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
        VisualCaptureSelection::PortalWindow | VisualCaptureSelection::PortalDisplay
    );
    if portal == cfg!(target_os = "linux") {
        Ok(())
    } else {
        Err(if cfg!(target_os = "linux") {
            "Choose a window or monitor using the desktop picker. Drawn regions are not available on Ubuntu yet."
        } else {
            "Choose an available Windows capture source."
        }.into())
    }
}

/// Called on a blocking startup worker, never the command/UI thread. Stop is
/// observed while the portal picker waits for its human response.
pub fn start_capture(
    selection: VisualCaptureSelection,
    parent_window: String,
    cancellation: CancellationToken,
) -> Result<StartedVisualCapture, StartError> {
    validate_selection(&selection).map_err(StartError::Failed)?;
    if cancellation.is_cancelled() {
        return Err(StartError::Cancelled);
    }
    #[cfg(target_os = "linux")]
    {
        use prollyglot_visual_pipewire::{CaptureOptions, PortalSource};
        let (source, label) = match selection {
            VisualCaptureSelection::PortalWindow => (PortalSource::Window, "Shared window"),
            VisualCaptureSelection::PortalDisplay => (PortalSource::Monitor, "Shared monitor"),
            _ => unreachable!("validated portal selection"),
        };
        let mut session = prollyglot_visual_pipewire::start_capture(CaptureOptions {
            source,
            parent_window,
        })
        .map_err(|error| StartError::Failed(error.to_string()))?;
        let events = CaptureEvents::Portal {
            events: session.events.clone(),
            label: label.into(),
        };
        let result = wait_for_geometry(&events, &cancellation);
        match result {
            Ok(source) => Ok(StartedVisualCapture {
                source,
                frames: session.frames.clone(),
                events,
                session,
            }),
            Err(error) => {
                // Releasing the portal request/session is part of startup, so
                // supervision cannot declare Stop complete before this joins.
                session
                    .stop()
                    .map_err(|error| StartError::Failed(error.to_string()))?;
                Err(error)
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = parent_window;
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

#[cfg(target_os = "linux")]
fn wait_for_geometry(
    events: &CaptureEvents,
    cancellation: &CancellationToken,
) -> Result<PickedVisualSource, StartError> {
    loop {
        if cancellation.is_cancelled() {
            return Err(StartError::Cancelled);
        }
        match events.recv_timeout(Duration::from_millis(20)) {
            Ok(VisualCaptureEvent::Started(source)) => return Ok(source),
            Ok(VisualCaptureEvent::SourceClosed) => return Err(StartError::Cancelled),
            Ok(VisualCaptureEvent::Failed(error)) => return Err(StartError::Failed(error)),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(StartError::Failed(
                    "Screen sharing ended before capture started.".into(),
                ));
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "linux")]
fn portal_geometry(
    label: &str,
    geometry: prollyglot_visual_pipewire::FrameGeometry,
) -> PickedVisualSource {
    PickedVisualSource {
        label: label.into(),
        x: 0,
        y: 0,
        width: geometry.width,
        height: geometry.height,
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use prollyglot_application_runtime::{
        SessionMode, SessionSource, SessionSourceKind, SessionSupervisor, StartSessionRequest,
    };
    use prollyglot_visual_pipeline::PixelRect;
    use prollyglot_visual_pipewire::{CaptureEvent, FrameGeometry, PortalSource, StreamInfo};

    fn token() -> CancellationToken {
        SessionSupervisor::default()
            .start(StartSessionRequest {
                mode: SessionMode::VisualTranslation,
                source: SessionSource::new("portal", SessionSourceKind::Display, "Monitor"),
            })
            .unwrap()
            .cancellation
    }

    #[test]
    fn portal_startup_waits_for_pixels_and_ignores_logical_desktop_coordinates() {
        let (sender, receiver) = crossbeam_channel::bounded(4);
        sender
            .send(CaptureEvent::Selected(StreamInfo {
                source: PortalSource::Monitor,
                logical_position: Some((-1920, 200)),
                logical_size: Some((1920, 1080)),
            }))
            .unwrap();
        sender
            .send(CaptureEvent::FormatChanged(FrameGeometry {
                raw_width: 3840,
                raw_height: 2160,
                crop: PixelRect::full(3840, 2160),
                transform: 1,
                width: 2160,
                height: 3840,
            }))
            .unwrap();
        let source = wait_for_geometry(
            &CaptureEvents::Portal {
                events: receiver,
                label: "Monitor".into(),
            },
            &token(),
        )
        .unwrap();
        assert_eq!(
            (source.x, source.y, source.width, source.height),
            (0, 0, 2160, 3840)
        );
    }

    #[test]
    fn picker_cancellation_and_failure_cannot_create_a_running_capture() {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let events = CaptureEvents::Portal {
            events: receiver,
            label: "Window".into(),
        };
        let cancellation = token();
        cancellation.cancel();
        assert!(matches!(
            wait_for_geometry(&events, &cancellation),
            Err(StartError::Cancelled)
        ));
        sender.send(CaptureEvent::Cancelled).unwrap();
        assert!(matches!(
            wait_for_geometry(&events, &token()),
            Err(StartError::Cancelled)
        ));
        sender
            .send(CaptureEvent::Failed("portal unavailable".into()))
            .unwrap();
        assert!(
            matches!(wait_for_geometry(&events, &token()), Err(StartError::Failed(message)) if message == "portal unavailable")
        );
        drop(sender);
        assert!(matches!(
            wait_for_geometry(&events, &token()),
            Err(StartError::Failed(_))
        ));
    }

    #[test]
    fn ubuntu_rejects_cached_windows_ids_and_drawn_regions() {
        assert!(validate_selection(&VisualCaptureSelection::PortalWindow).is_ok());
        assert!(validate_selection(&VisualCaptureSelection::PortalDisplay).is_ok());
        assert!(
            validate_selection(&VisualCaptureSelection::Display {
                source_id: "display:1".into()
            })
            .is_err()
        );
    }
}
