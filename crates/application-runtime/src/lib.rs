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
    ErrorRecoverability, RecoveryAction, RuntimeBootstrap, RuntimeHealth, RuntimeSnapshot,
    RuntimeStateEvent, SessionHealthLevel, SessionId, SessionLifecycle, SessionMode,
    SessionProgress, SessionSource, SessionSourceKind, StartSessionRequest,
};
pub use supervisor::{
    CancellationToken, SessionSupervisor, StartPermit, StopPermit, WorkerLifetime, WorkerOutcome,
    WorkerReporter, WorkerRole,
};

/// Stable IPC names shared by the native adapter and generated TypeScript.
pub mod ipc {
    pub const BOOTSTRAP_COMMAND: &str = "runtime_bootstrap";
    pub const STATE_EVENT: &str = "runtime-state";
}
