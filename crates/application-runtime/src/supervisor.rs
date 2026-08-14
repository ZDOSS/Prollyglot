use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};

use crate::{
    ApplicationError, ApplicationErrorCode, ErrorRecoverability, RecoveryAction, RuntimeHealth,
    RuntimeSnapshot, SessionHealthLevel, SessionId, SessionLifecycle, SessionProgress,
    StartSessionRequest,
};

#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug)]
pub struct StartPermit {
    pub session_id: SessionId,
    pub cancellation: CancellationToken,
    pub snapshot: RuntimeSnapshot,
}

#[derive(Clone, Debug)]
pub struct StopPermit {
    pub session_id: SessionId,
    pub cancellation: CancellationToken,
    pub already_stopping: bool,
    pub snapshot: RuntimeSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerLifetime {
    /// A finite worker such as model preparation. Successful completion is
    /// expected while the session continues starting.
    Startup,
    /// A worker expected to remain alive for the active session.
    Session,
    /// A finite worker responsible for cooperative shutdown and joining.
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerRole {
    ModelPreparation,
    Capture,
    CaptureEvents,
    Transcription,
    VisualRecognition,
    VisualEvents,
    EventForwarder,
    Shutdown,
}

impl fmt::Display for WorkerRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::ModelPreparation => "model preparation",
            Self::Capture => "capture",
            Self::CaptureEvents => "capture event",
            Self::Transcription => "transcription",
            Self::VisualRecognition => "visual recognition",
            Self::VisualEvents => "visual capture event",
            Self::EventForwarder => "event forwarding",
            Self::Shutdown => "shutdown",
        };
        formatter.write_str(label)
    }
}

#[derive(Clone, Debug)]
pub enum WorkerOutcome {
    Completed,
    Cancelled,
    Exited,
    Failed(ApplicationError),
    Panicked,
}

#[derive(Debug)]
struct WorkerCompletion {
    session_id: SessionId,
    worker_id: u64,
    outcome: WorkerOutcome,
}

/// Completion handle moved into a worker. Dropping it without an explicit
/// outcome reports an unexpected exit; dropping it while unwinding reports a
/// panic. This keeps detached thread failures visible to the supervisor.
#[derive(Debug)]
pub struct WorkerReporter {
    completion: Option<WorkerCompletion>,
    sender: Sender<WorkerCompletion>,
}

impl WorkerReporter {
    pub fn finish(mut self, outcome: WorkerOutcome) {
        if let Some(mut completion) = self.completion.take() {
            completion.outcome = outcome;
            let _ = self.sender.send(completion);
        }
    }
}

impl Drop for WorkerReporter {
    fn drop(&mut self) {
        let Some(mut completion) = self.completion.take() else {
            return;
        };
        completion.outcome = if std::thread::panicking() {
            WorkerOutcome::Panicked
        } else {
            WorkerOutcome::Exited
        };
        let _ = self.sender.send(completion);
    }
}

#[derive(Clone, Copy, Debug)]
struct WorkerRegistration {
    role: WorkerRole,
    lifetime: WorkerLifetime,
}

#[derive(Debug)]
struct ActiveSession {
    id: SessionId,
    cancellation: CancellationToken,
    workers: BTreeMap<u64, WorkerRegistration>,
    cleanup_finished: bool,
}

/// One authority for audio and visual session lifecycle.
#[derive(Debug)]
pub struct SessionSupervisor {
    snapshot: RuntimeSnapshot,
    active: Option<ActiveSession>,
    next_session_id: u32,
    next_worker_id: u64,
    completion_sender: Sender<WorkerCompletion>,
    completion_receiver: Receiver<WorkerCompletion>,
}

impl Default for SessionSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionSupervisor {
    pub fn new() -> Self {
        let (completion_sender, completion_receiver) = mpsc::channel();
        Self {
            snapshot: RuntimeSnapshot::default(),
            active: None,
            next_session_id: 1,
            next_worker_id: 1,
            completion_sender,
            completion_receiver,
        }
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.snapshot.clone()
    }

    pub fn has_active_session(&self) -> bool {
        self.active.is_some()
    }

    pub fn start(&mut self, request: StartSessionRequest) -> Result<StartPermit, ApplicationError> {
        if let Some(active) = self.active.as_ref() {
            return Err(ApplicationError::conflict(active.id));
        }

        let session_id = SessionId(self.next_session_id);
        self.next_session_id = self.next_session_id.checked_add(1).ok_or_else(|| {
            ApplicationError::new(
                ApplicationErrorCode::Internal,
                "The session identifier space was exhausted.",
                ErrorRecoverability::RestartRequired,
                RecoveryAction::RestartApplication,
            )
        })?;
        let cancellation = CancellationToken::new();
        self.active = Some(ActiveSession {
            id: session_id,
            cancellation: cancellation.clone(),
            workers: BTreeMap::new(),
            cleanup_finished: false,
        });
        let snapshot = self.publish(RuntimeSnapshot {
            session_id: Some(session_id),
            mode: Some(request.mode),
            source: Some(request.source),
            lifecycle: SessionLifecycle::Starting,
            health: RuntimeHealth::healthy(SessionProgress::PreparingModel, None),
            failure: None,
            ..RuntimeSnapshot::default()
        });
        Ok(StartPermit {
            session_id,
            cancellation,
            snapshot,
        })
    }

    pub fn update_start_progress(
        &mut self,
        session_id: SessionId,
        progress: SessionProgress,
        message: Option<String>,
    ) -> Result<RuntimeSnapshot, ApplicationError> {
        self.require_session(session_id)?;
        if self.snapshot.lifecycle != SessionLifecycle::Starting {
            return Err(ApplicationError::invalid_transition(
                session_id,
                self.snapshot.lifecycle,
                "update startup progress",
            ));
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.cancellation.is_cancelled())
        {
            return Err(ApplicationError::startup_cancelled(session_id));
        }
        let mut next = self.snapshot.clone();
        next.health = RuntimeHealth::healthy(progress, message);
        Ok(self.publish(next))
    }

    pub fn update_source(
        &mut self,
        session_id: SessionId,
        source: crate::SessionSource,
    ) -> Result<RuntimeSnapshot, ApplicationError> {
        self.require_session(session_id)?;
        if matches!(
            self.snapshot.lifecycle,
            SessionLifecycle::Stopping | SessionLifecycle::Failed | SessionLifecycle::Stopped
        ) {
            return Err(ApplicationError::invalid_transition(
                session_id,
                self.snapshot.lifecycle,
                "update the session source",
            ));
        }
        let mut next = self.snapshot.clone();
        next.source = Some(source);
        Ok(self.publish(next))
    }

    pub fn mark_running(
        &mut self,
        session_id: SessionId,
    ) -> Result<RuntimeSnapshot, ApplicationError> {
        self.require_session(session_id)?;
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.cancellation.is_cancelled())
        {
            return Err(ApplicationError::startup_cancelled(session_id));
        }
        if !matches!(
            self.snapshot.lifecycle,
            SessionLifecycle::Starting | SessionLifecycle::Waiting
        ) {
            return Err(ApplicationError::invalid_transition(
                session_id,
                self.snapshot.lifecycle,
                "mark the session running",
            ));
        }
        let mut next = self.snapshot.clone();
        next.lifecycle = SessionLifecycle::Running;
        next.health = RuntimeHealth::healthy(SessionProgress::Live, None);
        next.failure = None;
        Ok(self.publish(next))
    }

    pub fn mark_waiting(
        &mut self,
        session_id: SessionId,
        message: impl Into<String>,
    ) -> Result<RuntimeSnapshot, ApplicationError> {
        self.require_session(session_id)?;
        if !matches!(
            self.snapshot.lifecycle,
            SessionLifecycle::Starting | SessionLifecycle::Running | SessionLifecycle::Waiting
        ) {
            return Err(ApplicationError::invalid_transition(
                session_id,
                self.snapshot.lifecycle,
                "wait for the source",
            ));
        }
        let mut next = self.snapshot.clone();
        next.lifecycle = SessionLifecycle::Waiting;
        next.health = RuntimeHealth::recovering(message);
        Ok(self.publish(next))
    }

    pub fn update_health(
        &mut self,
        session_id: SessionId,
        level: SessionHealthLevel,
        message: Option<String>,
    ) -> Result<RuntimeSnapshot, ApplicationError> {
        self.require_session(session_id)?;
        if !matches!(
            self.snapshot.lifecycle,
            SessionLifecycle::Running | SessionLifecycle::Waiting
        ) {
            return Err(ApplicationError::invalid_transition(
                session_id,
                self.snapshot.lifecycle,
                "update runtime health",
            ));
        }
        let mut next = self.snapshot.clone();
        next.health = RuntimeHealth {
            level,
            progress: if self.snapshot.lifecycle == SessionLifecycle::Waiting {
                SessionProgress::WaitingForSource
            } else {
                SessionProgress::Live
            },
            message,
        };
        Ok(self.publish(next))
    }

    /// Stop is idempotent once stopping has begun. This lets every UI surface
    /// acknowledge one physical click without racing a second state owner.
    pub fn request_stop(
        &mut self,
        expected_session: Option<SessionId>,
    ) -> Result<StopPermit, ApplicationError> {
        let Some(active) = self.active.as_ref() else {
            return Err(ApplicationError::no_active_session());
        };
        if let Some(expected) = expected_session
            && expected != active.id
        {
            return Err(ApplicationError::stale_session(expected, active.id));
        }
        let session_id = active.id;
        let cancellation = active.cancellation.clone();
        cancellation.cancel();
        if self.snapshot.lifecycle == SessionLifecycle::Stopping {
            return Ok(StopPermit {
                session_id,
                cancellation,
                already_stopping: true,
                snapshot: self.snapshot.clone(),
            });
        }

        let mut next = self.snapshot.clone();
        next.lifecycle = SessionLifecycle::Stopping;
        next.health = RuntimeHealth::healthy(SessionProgress::Stopping, None);
        let snapshot = self.publish(next);
        Ok(StopPermit {
            session_id,
            cancellation,
            already_stopping: false,
            snapshot,
        })
    }

    pub fn request_stop_for_mode(
        &mut self,
        expected_mode: crate::SessionMode,
    ) -> Result<StopPermit, ApplicationError> {
        let Some(active) = self.active.as_ref() else {
            return Err(ApplicationError::no_active_session());
        };
        if self.snapshot.mode != Some(expected_mode) {
            return Err(ApplicationError::new(
                ApplicationErrorCode::SessionConflict,
                "A different Prollyglot session owns the runtime.",
                ErrorRecoverability::UserActionRequired,
                RecoveryAction::StopAndRetry,
            )
            .for_session(active.id));
        }
        self.request_stop(Some(active.id))
    }

    pub fn register_worker(
        &mut self,
        session_id: SessionId,
        role: WorkerRole,
        lifetime: WorkerLifetime,
    ) -> Result<WorkerReporter, ApplicationError> {
        self.require_session(session_id)?;
        let worker_id = self.next_worker_id;
        self.next_worker_id = self.next_worker_id.checked_add(1).ok_or_else(|| {
            ApplicationError::new(
                ApplicationErrorCode::Internal,
                "The worker identifier space was exhausted.",
                ErrorRecoverability::RestartRequired,
                RecoveryAction::RestartApplication,
            )
            .for_session(session_id)
        })?;
        self.active
            .as_mut()
            .expect("validated active session")
            .workers
            .insert(worker_id, WorkerRegistration { role, lifetime });
        Ok(WorkerReporter {
            completion: Some(WorkerCompletion {
                session_id,
                worker_id,
                outcome: WorkerOutcome::Completed,
            }),
            sender: self.completion_sender.clone(),
        })
    }

    /// Applies every completion currently waiting on the supervisor channel and
    /// returns only the public snapshots produced by those completions.
    pub fn drain_worker_completions(&mut self) -> Vec<RuntimeSnapshot> {
        let mut published = Vec::new();
        while let Ok(completion) = self.completion_receiver.try_recv() {
            if let Some(snapshot) = self.apply_worker_completion(completion) {
                published.push(snapshot);
            }
        }
        published
    }

    /// Signals that platform resources have been stopped and joined. The
    /// session reaches `Stopped` only after every registered worker has also
    /// reported a terminal outcome.
    pub fn finish_cleanup(
        &mut self,
        session_id: SessionId,
        result: Result<(), ApplicationError>,
    ) -> Result<Option<RuntimeSnapshot>, ApplicationError> {
        self.require_session(session_id)?;
        self.active
            .as_mut()
            .expect("validated active session")
            .cleanup_finished = true;
        if let Err(mut error) = result {
            if error.session_id.is_none() {
                error.session_id = Some(session_id);
            }
            let snapshot = self.fail_session(session_id, error)?;
            let _ = self.try_finalize_cleanup();
            return Ok(Some(snapshot));
        }
        Ok(self.try_finalize_cleanup())
    }

    pub fn fail(
        &mut self,
        session_id: SessionId,
        mut error: ApplicationError,
    ) -> Result<RuntimeSnapshot, ApplicationError> {
        if error.session_id.is_none() {
            error.session_id = Some(session_id);
        }
        self.fail_session(session_id, error)
    }

    pub fn shutdown_timed_out(
        &mut self,
        session_id: SessionId,
    ) -> Result<RuntimeSnapshot, ApplicationError> {
        self.fail_session(session_id, ApplicationError::shutdown_timed_out(session_id))
    }

    fn require_session(&self, expected: SessionId) -> Result<(), ApplicationError> {
        let Some(active) = self.active.as_ref() else {
            return Err(ApplicationError::no_active_session());
        };
        if active.id != expected {
            return Err(ApplicationError::stale_session(expected, active.id));
        }
        Ok(())
    }

    fn fail_session(
        &mut self,
        session_id: SessionId,
        error: ApplicationError,
    ) -> Result<RuntimeSnapshot, ApplicationError> {
        self.require_session(session_id)?;
        self.active
            .as_ref()
            .expect("validated active session")
            .cancellation
            .cancel();
        let mut next = self.snapshot.clone();
        next.lifecycle = SessionLifecycle::Failed;
        next.health = RuntimeHealth::degraded(error.message.clone());
        next.failure = Some(error);
        Ok(self.publish(next))
    }

    fn apply_worker_completion(&mut self, completion: WorkerCompletion) -> Option<RuntimeSnapshot> {
        let registration = {
            let active = self.active.as_mut()?;
            if active.id != completion.session_id {
                return None;
            }
            active.workers.remove(&completion.worker_id)?
        };
        let lifecycle = self.snapshot.lifecycle;
        let unexpected_session_exit = registration.lifetime == WorkerLifetime::Session
            && !matches!(
                lifecycle,
                SessionLifecycle::Stopping | SessionLifecycle::Failed
            );
        let unexpected_cancellation = matches!(completion.outcome, WorkerOutcome::Cancelled)
            && !matches!(
                lifecycle,
                SessionLifecycle::Stopping | SessionLifecycle::Failed
            );

        let failure = match completion.outcome {
            WorkerOutcome::Completed if unexpected_session_exit => {
                Some(ApplicationError::worker_exited(
                    completion.session_id,
                    &registration.role.to_string(),
                ))
            }
            WorkerOutcome::Cancelled if unexpected_cancellation => {
                Some(ApplicationError::worker_exited(
                    completion.session_id,
                    &registration.role.to_string(),
                ))
            }
            WorkerOutcome::Exited => Some(ApplicationError::worker_exited(
                completion.session_id,
                &registration.role.to_string(),
            )),
            WorkerOutcome::Failed(mut error) => {
                if error.session_id.is_none() {
                    error.session_id = Some(completion.session_id);
                }
                Some(error)
            }
            WorkerOutcome::Panicked => Some(ApplicationError::worker_panicked(
                completion.session_id,
                &registration.role.to_string(),
            )),
            WorkerOutcome::Completed | WorkerOutcome::Cancelled => None,
        };

        if let Some(error) = failure {
            return self.fail_session(completion.session_id, error).ok();
        }
        self.try_finalize_cleanup()
    }

    fn try_finalize_cleanup(&mut self) -> Option<RuntimeSnapshot> {
        let ready = self
            .active
            .as_ref()
            .is_some_and(|active| active.cleanup_finished && active.workers.is_empty());
        if !ready {
            return None;
        }
        let lifecycle = self.snapshot.lifecycle;
        self.active = None;
        if lifecycle != SessionLifecycle::Stopping {
            // Preserve a terminal failure after its resources are gone. A new
            // start is now legal and will replace this snapshot.
            return None;
        }
        Some(self.publish(RuntimeSnapshot::default()))
    }

    fn publish(&mut self, mut next: RuntimeSnapshot) -> RuntimeSnapshot {
        next.revision = self
            .snapshot
            .revision
            .checked_add(1)
            .expect("runtime revision space exhausted");
        self.snapshot = next.clone();
        next
    }

    #[cfg(test)]
    fn active_mode(&self) -> Option<crate::SessionMode> {
        self.active.as_ref()?;
        self.snapshot.mode
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;
    use crate::{SessionMode, SessionSource, SessionSourceKind, StartSessionRequest};

    fn audio_request() -> StartSessionRequest {
        StartSessionRequest {
            mode: SessionMode::AudioCaptions,
            source: SessionSource::new(
                "default-output",
                SessionSourceKind::SystemOutput,
                "System default",
            ),
        }
    }

    fn visual_request() -> StartSessionRequest {
        StartSessionRequest {
            mode: SessionMode::VisualTranslation,
            source: SessionSource::new("display:primary", SessionSourceKind::Display, "Display 1"),
        }
    }

    #[test]
    fn one_supervisor_excludes_audio_and_visual_sessions() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(audio_request()).expect("start audio");

        let error = supervisor
            .start(visual_request())
            .expect_err("visual start must conflict");

        assert_eq!(error.code, ApplicationErrorCode::SessionConflict);
        assert_eq!(error.session_id, Some(started.session_id));
        assert_eq!(supervisor.snapshot().revision, 1);
        assert_eq!(supervisor.active_mode(), Some(SessionMode::AudioCaptions));
    }

    #[test]
    fn cancellation_exists_before_model_loading_and_stop_is_idempotent() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(audio_request()).expect("start audio");
        assert!(!started.cancellation.is_cancelled());

        let first = supervisor
            .request_stop(Some(started.session_id))
            .expect("request stop");
        assert!(first.cancellation.is_cancelled());
        assert!(!first.already_stopping);
        assert_eq!(first.snapshot.lifecycle, SessionLifecycle::Stopping);

        let second = supervisor
            .request_stop(Some(started.session_id))
            .expect("repeat stop");
        assert!(second.already_stopping);
        assert_eq!(second.snapshot.revision, first.snapshot.revision);

        let stopped = supervisor
            .finish_cleanup(started.session_id, Ok(()))
            .expect("finish cleanup")
            .expect("publish stopped state");
        assert_eq!(stopped.lifecycle, SessionLifecycle::Stopped);
        assert_eq!(stopped.revision, 3);
        assert!(!supervisor.has_active_session());
    }

    #[test]
    fn legal_transitions_are_monotonic() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(audio_request()).expect("start audio");
        let running = supervisor
            .mark_running(started.session_id)
            .expect("mark running");
        let waiting = supervisor
            .mark_waiting(
                started.session_id,
                "The application closed; waiting for it to return.",
            )
            .expect("mark waiting");
        let resumed = supervisor
            .mark_running(started.session_id)
            .expect("resume running");

        assert_eq!(
            [
                started.snapshot.revision,
                running.revision,
                waiting.revision,
                resumed.revision,
            ],
            [1, 2, 3, 4]
        );
        assert_eq!(waiting.lifecycle, SessionLifecycle::Waiting);
        assert_eq!(waiting.health.level, SessionHealthLevel::Recovering);
        assert_eq!(resumed.health.progress, SessionProgress::Live);
    }

    #[test]
    fn a_resolved_source_updates_identity_without_changing_lifecycle() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(visual_request()).expect("start visual");
        let resolved = SessionSource::new(
            "display:primary",
            SessionSourceKind::Display,
            "Display 1 · Primary",
        );

        let updated = supervisor
            .update_source(started.session_id, resolved.clone())
            .expect("update source");

        assert_eq!(updated.lifecycle, SessionLifecycle::Starting);
        assert_eq!(updated.source, Some(resolved));
        assert_eq!(updated.revision, started.snapshot.revision + 1);
    }

    #[test]
    fn stop_during_start_prevents_a_late_running_transition() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(audio_request()).expect("start audio");
        supervisor
            .request_stop(Some(started.session_id))
            .expect("request stop");

        let error = supervisor
            .mark_running(started.session_id)
            .expect_err("cancelled startup cannot become live");

        assert_eq!(error.code, ApplicationErrorCode::StartupCancelled);
        assert_eq!(supervisor.snapshot().lifecycle, SessionLifecycle::Stopping);
    }

    #[test]
    fn a_mode_specific_stop_cannot_cancel_the_other_mode() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(audio_request()).expect("start audio");

        let error = supervisor
            .request_stop_for_mode(crate::SessionMode::VisualTranslation)
            .expect_err("visual stop must not cancel audio");

        assert_eq!(error.code, ApplicationErrorCode::SessionConflict);
        assert!(!started.cancellation.is_cancelled());
        assert_eq!(supervisor.snapshot().lifecycle, SessionLifecycle::Starting);
    }

    #[test]
    fn stale_session_updates_cannot_replace_the_current_session() {
        let mut supervisor = SessionSupervisor::new();
        let first = supervisor.start(audio_request()).expect("start first");
        supervisor
            .request_stop(Some(first.session_id))
            .expect("stop first");
        supervisor
            .finish_cleanup(first.session_id, Ok(()))
            .expect("finish first");
        let second = supervisor.start(visual_request()).expect("start second");
        let revision = second.snapshot.revision;

        let error = supervisor
            .mark_waiting(first.session_id, "old event")
            .expect_err("old session is stale");

        assert_eq!(error.code, ApplicationErrorCode::StaleSession);
        assert_eq!(supervisor.snapshot().revision, revision);
        assert_eq!(supervisor.snapshot().session_id, Some(second.session_id));
    }

    #[test]
    fn an_unexpected_session_worker_exit_becomes_a_typed_failure() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(audio_request()).expect("start audio");
        supervisor
            .mark_running(started.session_id)
            .expect("mark running");
        let reporter = supervisor
            .register_worker(
                started.session_id,
                WorkerRole::Transcription,
                WorkerLifetime::Session,
            )
            .expect("register worker");
        reporter.finish(WorkerOutcome::Completed);

        let updates = supervisor.drain_worker_completions();

        assert_eq!(updates.len(), 1);
        let failed = &updates[0];
        assert_eq!(failed.lifecycle, SessionLifecycle::Failed);
        assert_eq!(
            failed.failure.as_ref().map(|failure| failure.code),
            Some(ApplicationErrorCode::WorkerExited)
        );
        assert!(started.cancellation.is_cancelled());
    }

    #[test]
    fn cleanup_after_failure_allows_a_new_session_without_erasing_the_failure_revision() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(audio_request()).expect("start audio");
        supervisor
            .mark_running(started.session_id)
            .expect("mark running");
        let reporter = supervisor
            .register_worker(
                started.session_id,
                WorkerRole::Capture,
                WorkerLifetime::Session,
            )
            .expect("register capture");
        reporter.finish(WorkerOutcome::Panicked);
        let failed = supervisor
            .drain_worker_completions()
            .pop()
            .expect("publish failure");

        assert_eq!(failed.lifecycle, SessionLifecycle::Failed);
        assert!(
            supervisor
                .finish_cleanup(started.session_id, Ok(()))
                .expect("finish failed cleanup")
                .is_none()
        );
        assert!(!supervisor.has_active_session());
        assert_eq!(supervisor.snapshot().revision, failed.revision);

        let recovered = supervisor
            .start(visual_request())
            .expect("start replacement");
        assert_eq!(recovered.snapshot.revision, failed.revision + 1);
        assert_eq!(recovered.snapshot.lifecycle, SessionLifecycle::Starting);
        assert_eq!(recovered.snapshot.session_id, Some(recovered.session_id));
    }

    #[test]
    fn a_panicking_worker_reports_a_terminal_failure() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(visual_request()).expect("start visual");
        supervisor
            .mark_running(started.session_id)
            .expect("mark running");
        let reporter = supervisor
            .register_worker(
                started.session_id,
                WorkerRole::VisualRecognition,
                WorkerLifetime::Session,
            )
            .expect("register worker");
        let panicked = thread::spawn(move || {
            let _reporter = reporter;
            panic!("simulated OCR panic");
        })
        .join();
        assert!(panicked.is_err());

        let updates = supervisor.drain_worker_completions();

        assert_eq!(updates.len(), 1);
        assert_eq!(
            updates[0].failure.as_ref().map(|failure| failure.code),
            Some(ApplicationErrorCode::WorkerPanicked)
        );
    }

    #[test]
    fn stopping_waits_for_registered_workers_and_cleanup() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(audio_request()).expect("start audio");
        let reporter = supervisor
            .register_worker(
                started.session_id,
                WorkerRole::CaptureEvents,
                WorkerLifetime::Session,
            )
            .expect("register worker");
        supervisor
            .request_stop(Some(started.session_id))
            .expect("stop");

        assert!(
            supervisor
                .finish_cleanup(started.session_id, Ok(()))
                .expect("cleanup")
                .is_none()
        );
        assert!(supervisor.has_active_session());

        reporter.finish(WorkerOutcome::Cancelled);
        let updates = supervisor.drain_worker_completions();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].lifecycle, SessionLifecycle::Stopped);
        assert!(!supervisor.has_active_session());
    }

    #[test]
    fn shutdown_timeout_has_stable_recovery_guidance() {
        let mut supervisor = SessionSupervisor::new();
        let started = supervisor.start(audio_request()).expect("start audio");
        supervisor
            .request_stop(Some(started.session_id))
            .expect("stop");

        let failed = supervisor
            .shutdown_timed_out(started.session_id)
            .expect("timeout failure");
        let failure = failed.failure.expect("structured failure");
        assert_eq!(failure.code, ApplicationErrorCode::ShutdownTimedOut);
        assert_eq!(failure.recoverability, ErrorRecoverability::RestartRequired);
        assert_eq!(failure.suggested_action, RecoveryAction::RestartApplication);
    }
}
