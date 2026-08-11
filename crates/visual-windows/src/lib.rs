//! Windows visual capture behind a narrow, frame-oriented boundary.
//!
//! Captured pixels are delivered only through the in-process transient frame
//! channel. Events and diagnostics contain counters and geometry, never pixels.

#[cfg(target_os = "windows")]
mod platform;

use crossbeam_channel::Receiver;
use prollyglot_visual_pipeline::{PixelRect, VisualFrame};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualCaptureCapabilities {
    pub windows_graphics_capture: bool,
    pub system_picker: bool,
    pub desktop_duplication_experiment: bool,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VisualSourceKind {
    ApplicationWindow,
    Display,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualSource {
    pub id: String,
    pub kind: VisualSourceKind,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualSourceSnapshot {
    pub windows: Vec<VisualSource>,
    pub displays: Vec<VisualSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VisualCaptureSelection {
    ApplicationWindow {
        #[serde(rename = "sourceId")]
        source_id: String,
    },
    Display {
        #[serde(rename = "sourceId")]
        source_id: String,
    },
    Region {
        #[serde(rename = "displayId")]
        display_id: String,
        region: PixelRect,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedVisualSource {
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisualCaptureEvent {
    Started(PickedVisualSource),
    Frame {
        sequence: u64,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        replaced_frames: u64,
    },
    SourceClosed,
}

#[derive(Debug, Error)]
pub enum VisualCaptureError {
    #[error("visual capture is not supported on this platform")]
    UnsupportedPlatform,
    #[error("Windows Graphics Capture is unavailable on this system")]
    Unsupported,
    #[error("visual sources could not be enumerated: {0}")]
    Sources(String),
    #[error("the visual capture session could not start: {0}")]
    Start(String),
    #[error("the visual capture session could not stop: {0}")]
    Stop(String),
}

pub trait VisualCaptureSession: Send {
    fn stop(&mut self) -> Result<(), VisualCaptureError>;
}

pub struct StartedVisualCapture {
    pub source: PickedVisualSource,
    pub frames: Receiver<VisualFrame>,
    pub events: Receiver<VisualCaptureEvent>,
    session: Box<dyn VisualCaptureSession>,
}

impl StartedVisualCapture {
    pub fn stop(&mut self) -> Result<(), VisualCaptureError> {
        self.session.stop()
    }
}

#[cfg(target_os = "windows")]
pub fn capabilities() -> VisualCaptureCapabilities {
    platform::capabilities()
}

#[cfg(not(target_os = "windows"))]
pub fn capabilities() -> VisualCaptureCapabilities {
    VisualCaptureCapabilities {
        windows_graphics_capture: false,
        system_picker: false,
        desktop_duplication_experiment: false,
        message: Some("Visual capture is currently Windows-only.".into()),
    }
}

#[cfg(target_os = "windows")]
pub fn source_snapshot() -> Result<VisualSourceSnapshot, VisualCaptureError> {
    platform::source_snapshot()
}

#[cfg(not(target_os = "windows"))]
pub fn source_snapshot() -> Result<VisualSourceSnapshot, VisualCaptureError> {
    Err(VisualCaptureError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
pub fn start_capture(
    selection: VisualCaptureSelection,
) -> Result<StartedVisualCapture, VisualCaptureError> {
    platform::start_capture(selection)
}

#[cfg(not(target_os = "windows"))]
pub fn start_capture(
    _selection: VisualCaptureSelection,
) -> Result<StartedVisualCapture, VisualCaptureError> {
    Err(VisualCaptureError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn non_windows_capability_is_explicit() {
        let snapshot = capabilities();
        assert!(!snapshot.windows_graphics_capture);
        assert!(matches!(
            source_snapshot(),
            Err(VisualCaptureError::UnsupportedPlatform)
        ));
    }

    #[test]
    fn accepts_the_webview_selection_contract_for_every_source_kind() {
        let application: VisualCaptureSelection = serde_json::from_value(json!({
            "kind": "applicationWindow",
            "sourceId": "window:42"
        }))
        .expect("application selection");
        let display: VisualCaptureSelection = serde_json::from_value(json!({
            "kind": "display",
            "sourceId": "display:7"
        }))
        .expect("display selection");
        let region: VisualCaptureSelection = serde_json::from_value(json!({
            "kind": "region",
            "displayId": "display:7",
            "region": { "x": 12, "y": 34, "width": 640, "height": 360 }
        }))
        .expect("region selection");

        assert_eq!(
            application,
            VisualCaptureSelection::ApplicationWindow {
                source_id: "window:42".into()
            }
        );
        assert_eq!(
            display,
            VisualCaptureSelection::Display {
                source_id: "display:7".into()
            }
        );
        assert_eq!(
            region,
            VisualCaptureSelection::Region {
                display_id: "display:7".into(),
                region: PixelRect {
                    x: 12,
                    y: 34,
                    width: 640,
                    height: 360,
                }
            }
        );
    }

    #[test]
    fn publishes_selection_fields_in_camel_case() {
        let application = serde_json::to_value(VisualCaptureSelection::ApplicationWindow {
            source_id: "window:11".into(),
        })
        .expect("serialize application selection");
        let display = serde_json::to_value(VisualCaptureSelection::Display {
            source_id: "display:9".into(),
        })
        .expect("serialize display selection");
        let region = serde_json::to_value(VisualCaptureSelection::Region {
            display_id: "display:9".into(),
            region: PixelRect::full(1_920, 1_080),
        })
        .expect("serialize region selection");

        assert_eq!(application["kind"], "applicationWindow");
        assert_eq!(application["sourceId"], "window:11");
        assert!(application.get("source_id").is_none());
        assert_eq!(display["kind"], "display");
        assert_eq!(display["sourceId"], "display:9");
        assert!(display.get("source_id").is_none());
        assert_eq!(region["kind"], "region");
        assert_eq!(region["displayId"], "display:9");
        assert!(region.get("display_id").is_none());
    }
}
