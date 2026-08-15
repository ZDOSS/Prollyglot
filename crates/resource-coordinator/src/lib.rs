//! Session-scoped ownership for heavyweight local inference runtimes.

use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;
use prollyglot_application_runtime::{SessionId, SessionMode};
use thiserror::Error;

pub use prollyglot_application_runtime::InferenceResourceKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InferenceResourceRecord {
    pub session_id: SessionId,
    pub mode: SessionMode,
    pub kind: InferenceResourceKind,
    pub model_id: String,
    pub cold_start_millis: u64,
    pub resident_bytes_at_load: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InferenceResourceSnapshot {
    pub revision: u64,
    pub process_resident_bytes: Option<u64>,
    pub resources: Vec<InferenceResourceRecord>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InferenceResourceError {
    #[error(
        "session {requested_session} cannot load {requested_kind:?} while session {active_session} owns {active_kind:?}"
    )]
    SessionConflict {
        requested_session: SessionId,
        requested_kind: InferenceResourceKind,
        active_session: SessionId,
        active_kind: InferenceResourceKind,
    },
    #[error("session {session_id} already owns a {kind:?} inference runtime")]
    KindAlreadyLoaded {
        session_id: SessionId,
        kind: InferenceResourceKind,
    },
}

#[derive(Default)]
struct CoordinatorState {
    revision: u64,
    next_token: u64,
    resources: HashMap<u64, InferenceResourceRecord>,
}

#[derive(Clone, Default)]
pub struct InferenceResourceCoordinator {
    inner: Arc<Mutex<CoordinatorState>>,
}

impl InferenceResourceCoordinator {
    pub fn acquire(
        &self,
        record: InferenceResourceRecord,
    ) -> Result<InferenceResourceLease, InferenceResourceError> {
        let mut state = self.inner.lock();
        if let Some(active) = state
            .resources
            .values()
            .find(|active| active.session_id != record.session_id || active.mode != record.mode)
        {
            return Err(InferenceResourceError::SessionConflict {
                requested_session: record.session_id,
                requested_kind: record.kind,
                active_session: active.session_id,
                active_kind: active.kind,
            });
        }
        if state
            .resources
            .values()
            .any(|active| active.kind == record.kind)
        {
            return Err(InferenceResourceError::KindAlreadyLoaded {
                session_id: record.session_id,
                kind: record.kind,
            });
        }
        state.next_token = state.next_token.saturating_add(1).max(1);
        let token = state.next_token;
        tracing::info!(
            session_id = %record.session_id,
            mode = ?record.mode,
            kind = ?record.kind,
            model_id = %record.model_id,
            cold_start_ms = record.cold_start_millis,
            resident_bytes = record.resident_bytes_at_load,
            "local inference resource loaded"
        );
        state.resources.insert(token, record);
        state.revision = state.revision.saturating_add(1);
        drop(state);
        Ok(InferenceResourceLease {
            coordinator: self.clone(),
            token: Some(token),
        })
    }

    pub fn release_session(&self, session_id: SessionId) {
        let mut state = self.inner.lock();
        let before = state.resources.len();
        state.resources.retain(|_, resource| {
            let retain = resource.session_id != session_id;
            if !retain {
                tracing::info!(
                    session_id = %resource.session_id,
                    mode = ?resource.mode,
                    kind = ?resource.kind,
                    model_id = %resource.model_id,
                    "local inference resource force-released"
                );
            }
            retain
        });
        if state.resources.len() != before {
            state.revision = state.revision.saturating_add(1);
        }
    }

    pub fn snapshot(&self, process_resident_bytes: Option<u64>) -> InferenceResourceSnapshot {
        let state = self.inner.lock();
        let mut resources = state.resources.values().cloned().collect::<Vec<_>>();
        resources.sort_by_key(|resource| match resource.kind {
            InferenceResourceKind::Speech => 0,
            InferenceResourceKind::VisualOcr => 1,
            InferenceResourceKind::Translation => 2,
        });
        InferenceResourceSnapshot {
            revision: state.revision,
            process_resident_bytes,
            resources,
        }
    }

    fn release(&self, token: u64) {
        let mut state = self.inner.lock();
        if let Some(resource) = state.resources.remove(&token) {
            tracing::info!(
                session_id = %resource.session_id,
                mode = ?resource.mode,
                kind = ?resource.kind,
                model_id = %resource.model_id,
                "local inference resource unloaded"
            );
            state.revision = state.revision.saturating_add(1);
        }
    }
}

pub struct InferenceResourceLease {
    coordinator: InferenceResourceCoordinator,
    token: Option<u64>,
}

impl InferenceResourceLease {
    pub fn release(mut self) {
        if let Some(token) = self.token.take() {
            self.coordinator.release(token);
        }
    }
}

impl Drop for InferenceResourceLease {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            self.coordinator.release(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(
        session: u32,
        mode: SessionMode,
        kind: InferenceResourceKind,
    ) -> InferenceResourceRecord {
        InferenceResourceRecord {
            session_id: SessionId(session),
            mode,
            kind,
            model_id: format!("model-{kind:?}"),
            cold_start_millis: 25,
            resident_bytes_at_load: Some(128),
        }
    }

    #[test]
    fn one_session_can_own_its_primary_runtime_and_translator() {
        let coordinator = InferenceResourceCoordinator::default();
        let speech = coordinator
            .acquire(record(
                1,
                SessionMode::AudioCaptions,
                InferenceResourceKind::Speech,
            ))
            .expect("speech lease");
        let translation = coordinator
            .acquire(record(
                1,
                SessionMode::AudioCaptions,
                InferenceResourceKind::Translation,
            ))
            .expect("translation lease");
        assert_eq!(coordinator.snapshot(Some(512)).resources.len(), 2);
        drop((speech, translation));
        assert!(coordinator.snapshot(Some(256)).resources.is_empty());
    }

    #[test]
    fn a_different_session_or_mode_cannot_overlap_loaded_inference() {
        let coordinator = InferenceResourceCoordinator::default();
        let _speech = coordinator
            .acquire(record(
                3,
                SessionMode::AudioCaptions,
                InferenceResourceKind::Speech,
            ))
            .expect("speech lease");
        let error = coordinator
            .acquire(record(
                4,
                SessionMode::VisualTranslation,
                InferenceResourceKind::VisualOcr,
            ))
            .err()
            .expect("conflict");
        assert!(matches!(
            error,
            InferenceResourceError::SessionConflict { .. }
        ));
    }

    #[test]
    fn forced_session_cleanup_invalidates_late_lease_drops() {
        let coordinator = InferenceResourceCoordinator::default();
        let lease = coordinator
            .acquire(record(
                8,
                SessionMode::VisualTranslation,
                InferenceResourceKind::VisualOcr,
            ))
            .expect("OCR lease");
        coordinator.release_session(SessionId(8));
        let revision = coordinator.snapshot(None).revision;
        drop(lease);
        assert_eq!(coordinator.snapshot(None).revision, revision);
        assert!(coordinator.snapshot(None).resources.is_empty());
    }
}
