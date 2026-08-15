use parking_lot::Mutex;
use prollyglot_application_runtime::{RuntimeBootstrap, RuntimeSnapshot, RuntimeStateEvent};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::RuntimeState;

#[derive(Default)]
pub struct RuntimeEventPublisher {
    published_revision: Mutex<u32>,
}

impl RuntimeEventPublisher {
    fn publish(&self, app: &AppHandle, snapshot: &RuntimeSnapshot) -> bool {
        let mut published_revision = self.published_revision.lock();
        if !advance_revision(&mut published_revision, snapshot.revision) {
            tracing::debug!(
                revision = snapshot.revision,
                published_revision = *published_revision,
                "ignored an out-of-order runtime event"
            );
            return false;
        }
        tracing::info!(
            revision = snapshot.revision,
            session_id = snapshot.session_id.map_or(0, |session_id| session_id.0),
            session_active = snapshot.session_id.is_some(),
            mode = ?snapshot.mode,
            lifecycle = ?snapshot.lifecycle,
            progress = ?snapshot.health.progress,
            "runtime state changed"
        );
        if let Err(error) = app.emit(
            prollyglot_application_runtime::ipc::STATE_EVENT,
            RuntimeStateEvent {
                snapshot: snapshot.clone(),
            },
        ) {
            tracing::warn!(%error, "could not emit runtime state");
        }
        true
    }
}

fn advance_revision(published_revision: &mut u32, candidate: u32) -> bool {
    if candidate <= *published_revision {
        return false;
    }
    *published_revision = candidate;
    true
}

pub fn publish_snapshot(app: &AppHandle, snapshot: &RuntimeSnapshot) -> bool {
    app.state::<RuntimeState>()
        .runtime_events
        .publish(app, snapshot)
}

#[tauri::command]
pub fn runtime_bootstrap(state: State<'_, RuntimeState>) -> RuntimeBootstrap {
    RuntimeBootstrap {
        snapshot: state.supervisor.lock().snapshot(),
    }
}

#[cfg(test)]
mod tests {
    use super::advance_revision;

    #[test]
    fn revisions_must_advance_before_publication() {
        let mut published_revision = 0;

        assert!(advance_revision(&mut published_revision, 2));
        assert!(!advance_revision(&mut published_revision, 1));
        assert!(!advance_revision(&mut published_revision, 2));
        assert!(advance_revision(&mut published_revision, 3));
    }
}
