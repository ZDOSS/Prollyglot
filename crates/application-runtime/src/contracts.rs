use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const APPLICATION_RUNTIME_CONTRACT_VERSION: u16 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct SessionId(pub u32);

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum SessionMode {
    AudioCaptions,
    VisualTranslation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum SessionLifecycle {
    Stopped,
    Starting,
    Running,
    Waiting,
    Stopping,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum SessionSourceKind {
    SystemOutput,
    Application,
    InputDevice,
    ApplicationWindow,
    Display,
    Region,
}

/// Opaque application-level source identity. Native handles, paths, and PIDs do
/// not cross this boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct SessionSource {
    pub id: String,
    pub kind: SessionSourceKind,
    pub label: String,
}

impl SessionSource {
    pub fn new(id: impl Into<String>, kind: SessionSourceKind, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            label: label.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum SessionHealthLevel {
    Healthy,
    Recovering,
    Degraded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum SessionProgress {
    Idle,
    PreparingModel,
    StartingCapture,
    Live,
    WaitingForSource,
    Stopping,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RuntimeHealth {
    pub level: SessionHealthLevel,
    pub progress: SessionProgress,
    pub message: Option<String>,
}

impl RuntimeHealth {
    pub fn healthy(progress: SessionProgress, message: Option<String>) -> Self {
        Self {
            level: SessionHealthLevel::Healthy,
            progress,
            message,
        }
    }

    pub fn recovering(message: impl Into<String>) -> Self {
        Self {
            level: SessionHealthLevel::Recovering,
            progress: SessionProgress::WaitingForSource,
            message: Some(message.into()),
        }
    }

    pub fn degraded(message: impl Into<String>) -> Self {
        Self {
            level: SessionHealthLevel::Degraded,
            progress: SessionProgress::Failed,
            message: Some(message.into()),
        }
    }
}

impl Default for RuntimeHealth {
    fn default() -> Self {
        Self::healthy(SessionProgress::Idle, None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum ApplicationErrorCode {
    SessionConflict,
    NoActiveSession,
    StaleSession,
    InvalidTransition,
    StartupCancelled,
    WorkerExited,
    WorkerPanicked,
    ShutdownTimedOut,
    CaptureUnavailable,
    CaptureFailed,
    ModelUnavailable,
    ModelFailed,
    TranslationFailed,
    ConfigurationInvalid,
    WindowOperationFailed,
    Internal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum ErrorRecoverability {
    Automatic,
    Retryable,
    UserActionRequired,
    RestartRequired,
    NotRecoverable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum RecoveryAction {
    Retry,
    StopAndRetry,
    WaitForSource,
    ChooseAnotherSource,
    InstallModel,
    OpenSettings,
    RestartApplication,
    ReportIssue,
}

/// Stable error envelope returned by application commands and runtime events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ApplicationError {
    pub code: ApplicationErrorCode,
    pub message: String,
    pub recoverability: ErrorRecoverability,
    pub suggested_action: RecoveryAction,
    pub session_id: Option<SessionId>,
}

impl ApplicationError {
    pub fn new(
        code: ApplicationErrorCode,
        message: impl Into<String>,
        recoverability: ErrorRecoverability,
        suggested_action: RecoveryAction,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            recoverability,
            suggested_action,
            session_id: None,
        }
    }

    pub fn for_session(mut self, session_id: SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub(crate) fn conflict(active: SessionId) -> Self {
        Self::new(
            ApplicationErrorCode::SessionConflict,
            "Another Prollyglot session is already active.",
            ErrorRecoverability::UserActionRequired,
            RecoveryAction::StopAndRetry,
        )
        .for_session(active)
    }

    pub(crate) fn no_active_session() -> Self {
        Self::new(
            ApplicationErrorCode::NoActiveSession,
            "No Prollyglot session is active.",
            ErrorRecoverability::Retryable,
            RecoveryAction::Retry,
        )
    }

    pub(crate) fn stale_session(expected: SessionId, actual: SessionId) -> Self {
        Self::new(
            ApplicationErrorCode::StaleSession,
            format!("Session {expected} is no longer current; session {actual} owns the runtime."),
            ErrorRecoverability::Automatic,
            RecoveryAction::Retry,
        )
        .for_session(expected)
    }

    pub(crate) fn invalid_transition(
        session_id: SessionId,
        from: SessionLifecycle,
        operation: &str,
    ) -> Self {
        Self::new(
            ApplicationErrorCode::InvalidTransition,
            format!("Cannot {operation} while the session is {from:?}."),
            ErrorRecoverability::Retryable,
            RecoveryAction::Retry,
        )
        .for_session(session_id)
    }

    pub(crate) fn startup_cancelled(session_id: SessionId) -> Self {
        Self::new(
            ApplicationErrorCode::StartupCancelled,
            "Session startup was cancelled.",
            ErrorRecoverability::Retryable,
            RecoveryAction::Retry,
        )
        .for_session(session_id)
    }

    pub(crate) fn worker_exited(session_id: SessionId, role: &str) -> Self {
        Self::new(
            ApplicationErrorCode::WorkerExited,
            format!("The {role} worker stopped unexpectedly."),
            ErrorRecoverability::Retryable,
            RecoveryAction::StopAndRetry,
        )
        .for_session(session_id)
    }

    pub(crate) fn worker_panicked(session_id: SessionId, role: &str) -> Self {
        Self::new(
            ApplicationErrorCode::WorkerPanicked,
            format!("The {role} worker failed unexpectedly."),
            ErrorRecoverability::Retryable,
            RecoveryAction::StopAndRetry,
        )
        .for_session(session_id)
    }

    pub(crate) fn shutdown_timed_out(session_id: SessionId) -> Self {
        Self::new(
            ApplicationErrorCode::ShutdownTimedOut,
            "Prollyglot could not finish stopping the session in time.",
            ErrorRecoverability::RestartRequired,
            RecoveryAction::RestartApplication,
        )
        .for_session(session_id)
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ApplicationError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StartSessionRequest {
    pub mode: SessionMode,
    pub source: SessionSource,
}

// Session-facing desktop contracts live beside the lifecycle contract so the
// native adapter and TypeScript client consume one schema. Capture backends
// keep their own platform contracts and are converted at the adapter edge.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(tag = "kind", rename_all = "camelCase")]
pub enum CaptureSelection {
    SystemDefault,
    SystemOutput {
        #[serde(rename = "deviceId")]
        #[ts(rename = "deviceId")]
        device_id: String,
    },
    Application {
        #[serde(rename = "sourceId")]
        #[ts(rename = "sourceId")]
        source_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum CaptureState {
    Starting,
    Capturing,
    Waiting,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PlaybackDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ApplicationSource {
    pub id: String,
    pub name: String,
    pub instance_count: u32,
    pub device_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub playback_devices: Vec<PlaybackDevice>,
    pub applications: Vec<ApplicationSource>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct CaptureStatus {
    pub state: CaptureState,
    pub peak: f32,
    #[ts(type = "number")]
    pub dropped_frames: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub source_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub message: Option<String>,
}

impl Default for CaptureStatus {
    fn default() -> Self {
        Self {
            state: CaptureState::Stopped,
            peak: 0.0,
            dropped_frames: 0,
            source_label: None,
            message: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StartCaptureCommand {
    pub selection: CaptureSelection,
    pub language: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum VisualSourceKind {
    ApplicationWindow,
    Display,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualSource {
    pub id: String,
    pub kind: VisualSourceKind,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualSourceSnapshot {
    pub windows: Vec<VisualSource>,
    pub displays: Vec<VisualSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub fn fits_within(self, width: u32, height: u32) -> bool {
        self.width > 0
            && self.height > 0
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|right| right <= width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|bottom| bottom <= height)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualRegionSelectorRequest {
    pub display_id: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualRegionSelected {
    pub display_id: String,
    pub region: PixelRect,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ShowVisualRegionSelectorCommand {
    pub display_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct CompleteVisualRegionSelectionCommand {
    pub display_id: String,
    pub region: PixelRect,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(tag = "kind", rename_all = "camelCase")]
pub enum VisualCaptureSelection {
    ApplicationWindow {
        #[serde(rename = "sourceId")]
        #[ts(rename = "sourceId")]
        source_id: String,
    },
    Display {
        #[serde(rename = "sourceId")]
        #[ts(rename = "sourceId")]
        source_id: String,
    },
    Region {
        #[serde(rename = "displayId")]
        #[ts(rename = "displayId")]
        display_id: String,
        region: PixelRect,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum VisualDetectionMode {
    #[default]
    Focused,
    AllText,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualCaptureCapabilities {
    pub windows_graphics_capture: bool,
    pub system_picker: bool,
    pub desktop_duplication_experiment: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum VisualState {
    Starting,
    Capturing,
    Waiting,
    Stopping,
    #[default]
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualStatus {
    pub active: bool,
    pub state: VisualState,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub source_label: Option<String>,
    #[ts(type = "number")]
    pub frames_received: u64,
    #[ts(type = "number")]
    pub frames_analyzed: u64,
    #[ts(type = "number")]
    pub frames_unchanged: u64,
    #[ts(type = "number")]
    pub replaced_frames: u64,
    #[ts(type = "number")]
    pub visible_regions: u64,
    #[ts(type = "number")]
    pub overlay_regions: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualCaptureGeometry {
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl VisualRect {
    pub fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StableVisualTextRegion {
    #[ts(type = "number")]
    pub track_id: u64,
    #[ts(type = "number")]
    pub text_revision: u64,
    pub text: String,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub script: Option<String>,
    pub bounds: VisualRect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualTextUpdate {
    pub session_id: SessionId,
    pub runtime_revision: u32,
    pub source: VisualCaptureGeometry,
    pub visible: Vec<StableVisualTextRegion>,
    pub translation_requests: Vec<StableVisualTextRegion>,
    #[ts(type = "Array<number>")]
    pub removed_track_ids: Vec<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualTextClear {
    pub session_id: SessionId,
    pub runtime_revision: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum CaptionOutputMode {
    Original,
    Translated,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum CaptionPresentationPhase {
    Active,
    Holding,
    Cleared,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct CaptionPresentationEntry {
    pub key: String,
    pub source_language: String,
    pub original: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub translation: Option<String>,
    #[serde(default)]
    pub translation_pending: bool,
    pub is_final: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct CaptionPresentationFrame {
    pub session_id: SessionId,
    pub runtime_revision: u32,
    #[ts(type = "number")]
    pub presentation_revision: u64,
    pub phase: CaptionPresentationPhase,
    #[ts(type = "number")]
    pub readable_at_ms: u64,
    pub mode: CaptionOutputMode,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub target_language: Option<String>,
    pub entries: Vec<CaptionPresentationEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct UpdateCaptionPresentationCommand {
    pub frame: CaptionPresentationFrame,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualPresentationRegion {
    #[ts(type = "number")]
    pub track_id: u64,
    #[ts(type = "number")]
    pub text_revision: u64,
    pub original: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub translation: Option<String>,
    pub translation_pending: bool,
    #[serde(default)]
    pub retained: bool,
    pub bounds: VisualRect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualPresentationFrame {
    pub session_id: SessionId,
    pub runtime_revision: u32,
    #[ts(type = "number")]
    pub presentation_revision: u64,
    pub source_width: u32,
    pub source_height: u32,
    pub source_language: String,
    pub target_language: String,
    pub scanning: bool,
    pub regions: Vec<VisualPresentationRegion>,
}

impl Default for VisualPresentationFrame {
    fn default() -> Self {
        Self {
            session_id: SessionId(0),
            runtime_revision: 0,
            presentation_revision: 0,
            source_width: 1,
            source_height: 1,
            source_language: String::new(),
            target_language: String::new(),
            scanning: false,
            regions: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StartVisualTranslationCommand {
    pub selection: VisualCaptureSelection,
    pub source_language: String,
    pub target_language: String,
    pub detection_mode: Option<VisualDetectionMode>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct UpdateVisualPresentationCommand {
    pub frame: VisualPresentationFrame,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    pub contract_version: u16,
    pub revision: u32,
    pub session_id: Option<SessionId>,
    pub mode: Option<SessionMode>,
    pub source: Option<SessionSource>,
    pub lifecycle: SessionLifecycle,
    pub health: RuntimeHealth,
    pub failure: Option<ApplicationError>,
}

impl Default for RuntimeSnapshot {
    fn default() -> Self {
        Self {
            contract_version: APPLICATION_RUNTIME_CONTRACT_VERSION,
            revision: 0,
            session_id: None,
            mode: None,
            source: None,
            lifecycle: SessionLifecycle::Stopped,
            health: RuntimeHealth::default(),
            failure: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RuntimeBootstrap {
    pub snapshot: RuntimeSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RuntimeStateEvent {
    pub snapshot: RuntimeSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum InferenceResourceKind {
    Speech,
    VisualOcr,
    Translation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum InferenceResourcePhase {
    Loaded,
    Unloaded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ReportInferenceResourceCommand {
    pub session_id: SessionId,
    pub mode: SessionMode,
    pub owner_id: String,
    pub kind: InferenceResourceKind,
    pub phase: InferenceResourcePhase,
    pub model_id: Option<String>,
    #[ts(type = "number")]
    pub cold_start_millis: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct InferenceResourceStatus {
    pub session_id: SessionId,
    pub mode: SessionMode,
    pub kind: InferenceResourceKind,
    pub model_id: String,
    #[ts(type = "number")]
    pub cold_start_millis: u64,
    #[ts(type = "number | null")]
    pub resident_bytes_at_load: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct InferenceResourceSnapshot {
    #[ts(type = "number")]
    pub revision: u64,
    #[ts(type = "number | null")]
    pub process_resident_bytes: Option<u64>,
    pub resources: Vec<InferenceResourceStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_json_round_trip<T>(value: &T) -> serde_json::Value
    where
        T: Serialize + serde::de::DeserializeOwned + PartialEq + fmt::Debug,
    {
        let json = serde_json::to_value(value).expect("serialize desktop contract");
        let round_trip: T =
            serde_json::from_value(json.clone()).expect("deserialize desktop contract");
        assert_eq!(&round_trip, value);
        json
    }

    #[test]
    fn runtime_snapshot_uses_the_camel_case_wire_contract() {
        let snapshot = RuntimeSnapshot {
            revision: 7,
            session_id: Some(SessionId(3)),
            mode: Some(SessionMode::VisualTranslation),
            source: Some(SessionSource::new(
                "display:primary",
                SessionSourceKind::Display,
                "Display 1",
            )),
            lifecycle: SessionLifecycle::Running,
            health: RuntimeHealth::healthy(SessionProgress::Live, None),
            ..RuntimeSnapshot::default()
        };

        let value = serde_json::to_value(&snapshot).expect("serialize runtime snapshot");
        assert_eq!(
            value["contractVersion"],
            APPLICATION_RUNTIME_CONTRACT_VERSION
        );
        assert_eq!(value["sessionId"], 3);
        assert_eq!(value["mode"], "visualTranslation");
        assert_eq!(value["source"]["kind"], "display");
        assert_eq!(value["health"]["progress"], "live");

        let round_trip: RuntimeSnapshot =
            serde_json::from_value(value).expect("deserialize runtime snapshot");
        assert_eq!(round_trip, snapshot);
    }

    #[test]
    fn structured_error_keeps_recovery_metadata() {
        let error = ApplicationError::shutdown_timed_out(SessionId(12));
        let value = serde_json::to_value(error).expect("serialize application error");

        assert_eq!(value["code"], "shutdownTimedOut");
        assert_eq!(value["recoverability"], "restartRequired");
        assert_eq!(value["suggestedAction"], "restartApplication");
        assert_eq!(value["sessionId"], 12);
    }

    #[test]
    fn audio_session_contracts_round_trip_without_handwritten_field_names() {
        let command = StartCaptureCommand {
            selection: CaptureSelection::SystemOutput {
                device_id: "device:primary".into(),
            },
            language: "en".into(),
        };
        let command_json = assert_json_round_trip(&command);
        assert_eq!(command_json["selection"]["kind"], "systemOutput");
        assert_eq!(command_json["selection"]["deviceId"], "device:primary");

        let application_command = StartCaptureCommand {
            selection: CaptureSelection::Application {
                source_id: "app:example".into(),
            },
            language: "es".into(),
        };
        let application_json = assert_json_round_trip(&application_command);
        assert_eq!(application_json["selection"]["sourceId"], "app:example");
        assert!(application_json["selection"].get("processId").is_none());

        let sources = SourceSnapshot {
            playback_devices: vec![PlaybackDevice {
                id: "device:primary".into(),
                name: "Speakers".into(),
                is_default: true,
            }],
            applications: vec![ApplicationSource {
                id: "app:example".into(),
                name: "Player".into(),
                instance_count: 1,
                device_ids: vec!["device:primary".into()],
            }],
        };
        let sources_json = assert_json_round_trip(&sources);
        assert_eq!(sources_json["playbackDevices"][0]["isDefault"], true);
        assert_eq!(sources_json["applications"][0]["id"], "app:example");
        assert_eq!(sources_json["applications"][0]["instanceCount"], 1);

        let status = CaptureStatus {
            state: CaptureState::Capturing,
            peak: 0.5,
            dropped_frames: 3,
            source_label: Some("Speakers".into()),
            message: None,
        };
        let status_json = assert_json_round_trip(&status);
        assert_eq!(status_json["droppedFrames"], 3);
        assert!(status_json.get("message").is_none());
    }

    #[test]
    fn visual_session_contracts_round_trip_without_handwritten_field_names() {
        let command = StartVisualTranslationCommand {
            selection: VisualCaptureSelection::Region {
                display_id: "display:1".into(),
                region: PixelRect {
                    x: 10,
                    y: 20,
                    width: 640,
                    height: 360,
                },
            },
            source_language: "ja".into(),
            target_language: "en".into(),
            detection_mode: Some(VisualDetectionMode::Focused),
        };
        let command_json = assert_json_round_trip(&command);
        assert_eq!(command_json["selection"]["displayId"], "display:1");
        assert_eq!(command_json["sourceLanguage"], "ja");
        assert_eq!(command_json["detectionMode"], "focused");

        let selector = VisualRegionSelectorRequest {
            display_id: "display:1".into(),
            width: 1920,
            height: 1080,
        };
        assert_json_round_trip(&selector);
        assert_json_round_trip(&ShowVisualRegionSelectorCommand {
            display_id: selector.display_id.clone(),
        });
        assert_json_round_trip(&VisualRegionSelected {
            display_id: selector.display_id.clone(),
            region: PixelRect {
                x: 10,
                y: 20,
                width: 640,
                height: 360,
            },
        });
        assert_json_round_trip(&CompleteVisualRegionSelectionCommand {
            display_id: selector.display_id,
            region: PixelRect {
                x: 10,
                y: 20,
                width: 640,
                height: 360,
            },
        });

        let sources = VisualSourceSnapshot {
            windows: vec![VisualSource {
                id: "window:1".into(),
                kind: VisualSourceKind::ApplicationWindow,
                label: "News".into(),
                x: 5,
                y: 6,
                width: 1280,
                height: 720,
            }],
            displays: Vec::new(),
        };
        assert_json_round_trip(&sources);
        assert_json_round_trip(&VisualCaptureCapabilities {
            windows_graphics_capture: true,
            system_picker: false,
            desktop_duplication_experiment: false,
            message: None,
        });
        assert_json_round_trip(&VisualStatus {
            active: true,
            state: VisualState::Capturing,
            source_label: Some("News".into()),
            frames_received: 12,
            frames_analyzed: 4,
            frames_unchanged: 2,
            replaced_frames: 1,
            visible_regions: 1,
            overlay_regions: 1,
            message: None,
        });

        let region = StableVisualTextRegion {
            track_id: 7,
            text_revision: 2,
            text: "ニュース".into(),
            confidence: 0.9,
            language: Some("ja".into()),
            script: Some("Japanese".into()),
            bounds: VisualRect {
                x: 0.1,
                y: 0.2,
                width: 0.3,
                height: 0.1,
            },
        };
        let update = VisualTextUpdate {
            session_id: SessionId(9),
            runtime_revision: 14,
            source: VisualCaptureGeometry {
                label: "News".into(),
                x: 5,
                y: 6,
                width: 1280,
                height: 720,
            },
            visible: vec![region.clone()],
            translation_requests: vec![region],
            removed_track_ids: vec![3],
        };
        let update_json = assert_json_round_trip(&update);
        assert_eq!(update_json["sessionId"], 9);
        assert_eq!(update_json["translationRequests"][0]["trackId"], 7);

        let caption = UpdateCaptionPresentationCommand {
            frame: CaptionPresentationFrame {
                session_id: SessionId(9),
                runtime_revision: 14,
                presentation_revision: 3,
                phase: CaptionPresentationPhase::Holding,
                readable_at_ms: 1_750_000_000_000,
                mode: CaptionOutputMode::Both,
                target_language: Some("en".into()),
                entries: vec![CaptionPresentationEntry {
                    key: "ja:7".into(),
                    source_language: "ja".into(),
                    original: "ニュース".into(),
                    translation: Some("News".into()),
                    translation_pending: false,
                    is_final: true,
                }],
            },
        };
        let caption_json = assert_json_round_trip(&caption);
        assert_eq!(caption_json["frame"]["sessionId"], 9);
        assert_eq!(caption_json["frame"]["phase"], "holding");

        let output = UpdateVisualPresentationCommand {
            frame: VisualPresentationFrame {
                session_id: SessionId(9),
                runtime_revision: 14,
                presentation_revision: 4,
                source_width: 1280,
                source_height: 720,
                source_language: "ja".into(),
                target_language: "en".into(),
                scanning: false,
                regions: vec![VisualPresentationRegion {
                    track_id: 7,
                    text_revision: 2,
                    original: "ニュース".into(),
                    translation: Some("News".into()),
                    translation_pending: false,
                    retained: false,
                    bounds: VisualRect {
                        x: 0.1,
                        y: 0.2,
                        width: 0.3,
                        height: 0.1,
                    },
                }],
            },
        };
        let output_json = assert_json_round_trip(&output);
        assert_eq!(output_json["frame"]["regions"][0]["translation"], "News");
    }

    #[test]
    fn inference_resource_contracts_keep_session_ownership_and_diagnostics() {
        let command = ReportInferenceResourceCommand {
            session_id: SessionId(13),
            mode: SessionMode::AudioCaptions,
            owner_id: "captions:4".into(),
            kind: InferenceResourceKind::Translation,
            phase: InferenceResourcePhase::Loaded,
            model_id: Some("opus-ja-en".into()),
            cold_start_millis: 842,
        };
        let command_json = assert_json_round_trip(&command);
        assert_eq!(command_json["sessionId"], 13);
        assert_eq!(command_json["mode"], "audioCaptions");
        assert_eq!(command_json["ownerId"], "captions:4");
        assert_eq!(command_json["coldStartMillis"], 842);

        let snapshot = InferenceResourceSnapshot {
            revision: 5,
            process_resident_bytes: Some(512 * 1_024 * 1_024),
            resources: vec![InferenceResourceStatus {
                session_id: SessionId(13),
                mode: SessionMode::AudioCaptions,
                kind: InferenceResourceKind::Translation,
                model_id: "opus-ja-en".into(),
                cold_start_millis: 842,
                resident_bytes_at_load: Some(480 * 1_024 * 1_024),
            }],
        };
        let snapshot_json = assert_json_round_trip(&snapshot);
        assert_eq!(snapshot_json["resources"][0]["kind"], "translation");
        assert_eq!(snapshot_json["processResidentBytes"], 512 * 1_024 * 1_024);
    }
}
