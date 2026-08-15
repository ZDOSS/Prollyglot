use std::sync::Arc;

use parking_lot::Mutex;
use prollyglot_application_runtime::{
    ApplicationError, ApplicationErrorCode, ErrorRecoverability, InferenceResourceKind,
    InferenceResourcePhase, InferenceResourceSnapshot, InferenceResourceStatus, RecoveryAction,
    ReportInferenceResourceCommand, SessionId, SessionLifecycle, SessionMode,
};
use prollyglot_resource_coordinator::{
    InferenceResourceCoordinator, InferenceResourceLease, InferenceResourceRecord,
};
use tauri::State;

use crate::RuntimeState;

struct FrontendTranslationLease {
    session_id: SessionId,
    owner_id: String,
    model_id: String,
    _lease: InferenceResourceLease,
}

#[derive(Clone, Default)]
pub struct ResourceRuntime {
    coordinator: InferenceResourceCoordinator,
    translation: Arc<Mutex<Option<FrontendTranslationLease>>>,
}

impl ResourceRuntime {
    pub fn acquire(
        &self,
        session_id: SessionId,
        mode: SessionMode,
        kind: InferenceResourceKind,
        model_id: impl Into<String>,
        cold_start_millis: u64,
    ) -> Result<InferenceResourceLease, ApplicationError> {
        self.coordinator
            .acquire(InferenceResourceRecord {
                session_id,
                mode,
                kind,
                model_id: model_id.into(),
                cold_start_millis,
                resident_bytes_at_load: process_resident_bytes(),
            })
            .map_err(|error| resource_conflict(error.to_string(), session_id))
    }

    pub fn release_session(&self, session_id: SessionId) {
        let frontend_lease = {
            let mut translation = self.translation.lock();
            if translation
                .as_ref()
                .is_some_and(|lease| lease.session_id == session_id)
            {
                translation.take()
            } else {
                None
            }
        };
        drop(frontend_lease);
        self.coordinator.release_session(session_id);
    }

    pub fn snapshot(&self) -> InferenceResourceSnapshot {
        let snapshot = self.coordinator.snapshot(process_resident_bytes());
        InferenceResourceSnapshot {
            revision: snapshot.revision,
            process_resident_bytes: snapshot.process_resident_bytes,
            resources: snapshot
                .resources
                .into_iter()
                .map(|resource| InferenceResourceStatus {
                    session_id: resource.session_id,
                    mode: resource.mode,
                    kind: resource.kind,
                    model_id: resource.model_id,
                    cold_start_millis: resource.cold_start_millis,
                    resident_bytes_at_load: resource.resident_bytes_at_load,
                })
                .collect(),
        }
    }

    fn report_translation(
        &self,
        supervisor: &Mutex<prollyglot_application_runtime::SessionSupervisor>,
        command: ReportInferenceResourceCommand,
    ) -> Result<InferenceResourceSnapshot, ApplicationError> {
        if command.kind != InferenceResourceKind::Translation {
            return Err(resource_conflict(
                "Only the WebView translation runtime may report frontend inference ownership.",
                command.session_id,
            ));
        }

        match command.phase {
            InferenceResourcePhase::Loaded => {
                let snapshot = supervisor.lock().snapshot();
                if snapshot.session_id != Some(command.session_id)
                    || snapshot.mode != Some(command.mode)
                    || !matches!(
                        snapshot.lifecycle,
                        SessionLifecycle::Starting
                            | SessionLifecycle::Running
                            | SessionLifecycle::Waiting
                    )
                {
                    return Err(resource_conflict(
                        "The translation runtime reported a load for a session that is no longer active.",
                        command.session_id,
                    ));
                }
                let owner_id = normalized_identifier(&command.owner_id, "translation owner")?;
                let model_id = normalized_identifier(
                    command.model_id.as_deref().unwrap_or_default(),
                    "translation model",
                )?;
                let mut translation = self.translation.lock();
                if translation.as_ref().is_some_and(|lease| {
                    lease.session_id == command.session_id
                        && lease.owner_id == owner_id
                        && lease.model_id == model_id
                }) {
                    return Ok(self.snapshot());
                }
                if let Some(active) = translation.as_ref()
                    && (active.session_id != command.session_id || active.owner_id != owner_id)
                {
                    return Err(resource_conflict(
                        "A different translation runtime still owns the active inference slot.",
                        command.session_id,
                    ));
                }
                drop(translation.take());
                let lease = self.acquire(
                    command.session_id,
                    command.mode,
                    InferenceResourceKind::Translation,
                    model_id.clone(),
                    command.cold_start_millis,
                )?;
                *translation = Some(FrontendTranslationLease {
                    session_id: command.session_id,
                    owner_id,
                    model_id,
                    _lease: lease,
                });
            }
            InferenceResourcePhase::Unloaded => {
                let owner_id = normalized_identifier(&command.owner_id, "translation owner")?;
                let lease = {
                    let mut translation = self.translation.lock();
                    if translation.as_ref().is_some_and(|lease| {
                        lease.session_id == command.session_id && lease.owner_id == owner_id
                    }) {
                        translation.take()
                    } else {
                        None
                    }
                };
                drop(lease);
            }
        }
        Ok(self.snapshot())
    }
}

#[tauri::command]
pub fn inference_resource_status(state: State<'_, RuntimeState>) -> InferenceResourceSnapshot {
    state.resources.snapshot()
}

#[tauri::command]
pub fn report_inference_resource(
    state: State<'_, RuntimeState>,
    command: ReportInferenceResourceCommand,
) -> Result<InferenceResourceSnapshot, ApplicationError> {
    state
        .resources
        .report_translation(&state.supervisor, command)
}

fn normalized_identifier(value: &str, label: &str) -> Result<String, ApplicationError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 240 {
        return Err(ApplicationError::new(
            ApplicationErrorCode::ConfigurationInvalid,
            format!("The {label} identifier is invalid."),
            ErrorRecoverability::UserActionRequired,
            RecoveryAction::StopAndRetry,
        ));
    }
    Ok(value.to_owned())
}

fn resource_conflict(message: impl Into<String>, session_id: SessionId) -> ApplicationError {
    ApplicationError::new(
        ApplicationErrorCode::SessionConflict,
        message,
        ErrorRecoverability::Retryable,
        RecoveryAction::StopAndRetry,
    )
    .for_session(session_id)
}

#[cfg(target_os = "linux")]
fn process_resident_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            let kibibytes = line.strip_prefix("VmRSS:")?.split_whitespace().next()?;
            kibibytes.parse::<u64>().ok()?.checked_mul(1_024)
        })
}

#[cfg(target_os = "windows")]
fn process_resident_bytes() -> Option<u64> {
    use windows::Win32::System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).ok()?,
        ..Default::default()
    };
    unsafe {
        K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb)
            .as_bool()
            .then_some(counters.WorkingSetSize as u64)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn process_resident_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use prollyglot_application_runtime::{SessionSource, SessionSourceKind, StartSessionRequest};

    #[test]
    fn linux_resident_memory_probe_is_nonzero() {
        #[cfg(target_os = "linux")]
        assert!(process_resident_bytes().is_some_and(|bytes| bytes > 0));
    }

    #[test]
    fn frontend_translation_report_requires_the_matching_active_session() {
        let resources = ResourceRuntime::default();
        let supervisor = Mutex::new(prollyglot_application_runtime::SessionSupervisor::default());
        let started = supervisor
            .lock()
            .start(StartSessionRequest {
                mode: SessionMode::AudioCaptions,
                source: SessionSource::new(
                    "device:default",
                    SessionSourceKind::SystemOutput,
                    "Speakers",
                ),
            })
            .expect("start session");
        let command = ReportInferenceResourceCommand {
            session_id: started.session_id,
            mode: SessionMode::AudioCaptions,
            owner_id: "captions:1".into(),
            kind: InferenceResourceKind::Translation,
            phase: InferenceResourcePhase::Loaded,
            model_id: Some("opus-ja-en".into()),
            cold_start_millis: 320,
        };
        let snapshot = resources
            .report_translation(&supervisor, command.clone())
            .expect("track translation");
        assert_eq!(snapshot.resources.len(), 1);

        let stale = ReportInferenceResourceCommand {
            session_id: SessionId(started.session_id.0 + 1),
            ..command
        };
        assert!(resources.report_translation(&supervisor, stale).is_err());
    }

    #[test]
    fn stale_unload_cannot_release_a_new_translation_owner() {
        let resources = ResourceRuntime::default();
        let supervisor = Mutex::new(prollyglot_application_runtime::SessionSupervisor::default());
        let started = supervisor
            .lock()
            .start(StartSessionRequest {
                mode: SessionMode::VisualTranslation,
                source: SessionSource::new("display:1", SessionSourceKind::Display, "Display 1"),
            })
            .expect("start session");
        resources
            .report_translation(
                &supervisor,
                ReportInferenceResourceCommand {
                    session_id: started.session_id,
                    mode: SessionMode::VisualTranslation,
                    owner_id: "visual:2".into(),
                    kind: InferenceResourceKind::Translation,
                    phase: InferenceResourcePhase::Loaded,
                    model_id: Some("m2m100".into()),
                    cold_start_millis: 500,
                },
            )
            .expect("track translation");
        resources
            .report_translation(
                &supervisor,
                ReportInferenceResourceCommand {
                    session_id: started.session_id,
                    mode: SessionMode::VisualTranslation,
                    owner_id: "visual:1".into(),
                    kind: InferenceResourceKind::Translation,
                    phase: InferenceResourcePhase::Unloaded,
                    model_id: None,
                    cold_start_millis: 0,
                },
            )
            .expect("ignore stale unload");
        assert_eq!(resources.snapshot().resources.len(), 1);
    }
}
