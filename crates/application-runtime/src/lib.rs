//! Platform-neutral application orchestration for Prollyglot.
//!
//! This crate owns session identity, legal lifecycle transitions, cancellation,
//! worker completion, structured failures, and the wire contracts consumed by
//! the desktop adapters. It deliberately has no dependency on Tauri, capture
//! APIs, inference engines, or a running desktop session.

mod bindings;
mod contracts;
mod supervisor;

pub use bindings::typescript_bindings;
pub use contracts::{
    APPLICATION_RUNTIME_CONTRACT_VERSION, ApplicationError, ApplicationErrorCode,
    ApplicationSource, CaptureSelection, CaptureState, CaptureStatus,
    CompleteVisualRegionSelectionCommand, ErrorRecoverability, PixelRect, PlaybackDevice,
    RecoveryAction, RuntimeBootstrap, RuntimeHealth, RuntimeSnapshot, RuntimeStateEvent,
    SessionHealthLevel, SessionId, SessionLifecycle, SessionMode, SessionProgress, SessionSource,
    SessionSourceKind, ShowVisualRegionSelectorCommand, SourceSnapshot, StableVisualTextRegion,
    StartCaptureCommand, StartSessionRequest, StartVisualTranslationCommand,
    UpdateVisualOverlayOutputCommand, VisualCaptureCapabilities, VisualCaptureGeometry,
    VisualCaptureSelection, VisualDetectionMode, VisualOverlayOutput, VisualOverlayRegion,
    VisualRect, VisualRegionSelected, VisualRegionSelectorRequest, VisualSource, VisualSourceKind,
    VisualSourceSnapshot, VisualState, VisualStatus, VisualTextClear, VisualTextUpdate,
};
pub use supervisor::{
    CancellationToken, SessionSupervisor, StartPermit, StopPermit, WorkerLifetime, WorkerOutcome,
    WorkerReporter, WorkerRole,
};

/// Stable IPC names shared by the native adapter and generated TypeScript.
pub mod ipc {
    pub const BOOTSTRAP_COMMAND: &str = "runtime_bootstrap";
    pub const SOURCE_SNAPSHOT_COMMAND: &str = "source_snapshot";
    pub const START_CAPTURE_COMMAND: &str = "start_capture";
    pub const STOP_CAPTURE_COMMAND: &str = "stop_capture";
    pub const CAPTURE_STATUS_COMMAND: &str = "capture_status";
    pub const VISUAL_CAPABILITIES_COMMAND: &str = "visual_capabilities";
    pub const VISUAL_SOURCE_SNAPSHOT_COMMAND: &str = "visual_source_snapshot";
    pub const VISUAL_STATUS_COMMAND: &str = "visual_status";
    pub const SHOW_VISUAL_REGION_SELECTOR_COMMAND: &str = "show_visual_region_selector";
    pub const COMPLETE_VISUAL_REGION_SELECTION_COMMAND: &str = "complete_visual_region_selection";
    pub const CANCEL_VISUAL_REGION_SELECTION_COMMAND: &str = "cancel_visual_region_selection";
    pub const START_VISUAL_TRANSLATION_COMMAND: &str = "start_visual_translation";
    pub const STOP_VISUAL_TRANSLATION_COMMAND: &str = "stop_visual_translation";
    pub const UPDATE_VISUAL_OVERLAY_OUTPUT_COMMAND: &str = "update_visual_overlay_output";

    pub const STATE_EVENT: &str = "runtime-state";
    pub const CAPTURE_STATUS_EVENT: &str = "capture-status";
    pub const VISUAL_STATUS_EVENT: &str = "visual-status";
    pub const VISUAL_TEXT_EVENT: &str = "visual-text-update";
    pub const VISUAL_CLEAR_EVENT: &str = "visual-text-clear";
    pub const VISUAL_REGION_SELECTED_EVENT: &str = "visual-region-selected";
    pub const VISUAL_REGION_SELECTION_CANCELLED_EVENT: &str = "visual-region-selection-cancelled";
    pub const VISUAL_REGION_SELECTOR_REQUEST_EVENT: &str = "visual-region-selector-request";
}
