use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const APPLICATION_RUNTIME_CONTRACT_VERSION: u16 = 1;

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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(value["contractVersion"], 1);
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
}
