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
    ApplicationSource, CaptionOutputMode, CaptionPresentationEntry, CaptionPresentationFrame,
    CaptionPresentationPhase, CaptureSelection, CaptureState, CaptureStatus,
    CompleteVisualRegionSelectionCommand, ErrorRecoverability, InferenceResourceKind,
    InferenceResourcePhase, InferenceResourceSnapshot, InferenceResourceStatus, PixelRect,
    PlaybackDevice, RecoveryAction, ReportInferenceResourceCommand, RuntimeBootstrap,
    RuntimeHealth, RuntimeSnapshot, RuntimeStateEvent, SessionHealthLevel, SessionId,
    SessionLifecycle, SessionMode, SessionProgress, SessionSource, SessionSourceKind,
    ShowVisualRegionSelectorCommand, SourceSnapshot, StableVisualTextRegion, StartCaptureCommand,
    StartSessionRequest, StartVisualTranslationCommand, UpdateCaptionPresentationCommand,
    UpdateVisualPresentationCommand, VisualCaptureCapabilities, VisualCaptureGeometry,
    VisualCaptureSelection, VisualDetectionMode, VisualPresentationFrame, VisualPresentationRegion,
    VisualRect, VisualRegionSelected, VisualRegionSelectorRequest, VisualSource, VisualSourceKind,
    VisualSourceSnapshot, VisualState, VisualStatus, VisualTextClear, VisualTextUpdate,
};
pub use prollyglot_config::{
    ApplicationConfiguration, AudioSourcePreference, BilingualLayout, CONFIGURATION_SCHEMA_VERSION,
    CaptionOutputPreference, CaptionPreferences, ConfigurationSnapshot, ModelPreferences,
    OverlayPosition, OverlaySettings, UpdateConfigurationCommand, ViewMode,
    VisualDetectionPreference, VisualPreferences, VisualSourcePreference,
};
pub use supervisor::{
    CancellationToken, SessionSupervisor, StartPermit, StopPermit, WorkerLifetime, WorkerOutcome,
    WorkerReporter, WorkerRole,
};

/// Stable IPC names shared by the native adapter and generated TypeScript.
pub mod ipc {
    pub const CONFIGURATION_SNAPSHOT_COMMAND: &str = "configuration_snapshot";
    pub const UPDATE_CONFIGURATION_COMMAND: &str = "update_configuration";
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
    pub const UPDATE_CAPTION_PRESENTATION_COMMAND: &str = "update_caption_presentation";
    pub const UPDATE_VISUAL_PRESENTATION_COMMAND: &str = "update_visual_presentation";
    pub const INFERENCE_RESOURCE_STATUS_COMMAND: &str = "inference_resource_status";
    pub const REPORT_INFERENCE_RESOURCE_COMMAND: &str = "report_inference_resource";

    pub const STATE_EVENT: &str = "runtime-state";
    pub const CAPTURE_STATUS_EVENT: &str = "capture-status";
    pub const VISUAL_STATUS_EVENT: &str = "visual-status";
    pub const VISUAL_TEXT_EVENT: &str = "visual-text-update";
    pub const VISUAL_CLEAR_EVENT: &str = "visual-text-clear";
    pub const VISUAL_REGION_SELECTED_EVENT: &str = "visual-region-selected";
    pub const VISUAL_REGION_SELECTION_CANCELLED_EVENT: &str = "visual-region-selection-cancelled";
    pub const VISUAL_REGION_SELECTOR_REQUEST_EVENT: &str = "visual-region-selector-request";
    pub const CAPTION_PRESENTATION_EVENT: &str = "caption-presentation";
    pub const VISUAL_PRESENTATION_EVENT: &str = "visual-presentation";
    pub const CONFIGURATION_EVENT: &str = "configuration-updated";
}
