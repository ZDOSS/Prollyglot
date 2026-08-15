use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{RecvTimeoutError, TrySendError};
use parking_lot::Mutex;
use prollyglot_application_runtime::{
    ApplicationError, ApplicationErrorCode, ApplicationSource, CancellationToken, CaptureSelection,
    CaptureState, CaptureStatus, ErrorRecoverability, PlaybackDevice, RecoveryAction,
    RuntimeSnapshot, SessionHealthLevel, SessionId, SessionLifecycle, SessionMode, SessionProgress,
    SessionSource, SessionSourceKind, SourceSnapshot, StartSessionRequest, WorkerLifetime,
    WorkerOutcome, WorkerReporter, WorkerRole,
};
use prollyglot_core::{
    AudioCaptureBackend, AudioFrame, CaptureEvent, CaptureSelection as BackendCaptureSelection,
    CaptureSession, CaptureState as BackendCaptureState, ResolvedCaptureSelection,
};
use tauri::{AppHandle, Emitter, State};

use crate::{RuntimeState, show_live_overlay};

const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(25);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);
const INFERENCE_QUEUE_CAPACITY: usize = 128;
const INFERENCE_QUEUE_RECOVERY_DEPTH: usize = INFERENCE_QUEUE_CAPACITY / 4;
const INFERENCE_BACKLOG_RECOVERY_GRACE: Duration = Duration::from_secs(2);

#[derive(Default)]
struct PublishedAudioStatus {
    runtime_revision: u32,
    capture: CaptureStatus,
}

impl PublishedAudioStatus {
    fn apply_snapshot(&mut self, snapshot: &RuntimeSnapshot) -> Option<CaptureStatus> {
        if snapshot.revision <= self.runtime_revision {
            return None;
        }
        let next = projected_status(&self.capture, snapshot);
        self.runtime_revision = snapshot.revision;
        self.capture = next.clone();
        Some(next)
    }
}

type SharedAudioStatus = Arc<Mutex<PublishedAudioStatus>>;

struct ActiveAudioResources {
    session_id: SessionId,
    capture: Box<dyn CaptureSession>,
    event_forwarder: Option<JoinHandle<()>>,
    transcription_worker: Option<JoinHandle<()>>,
}

struct AudioEventForwarder {
    app: AppHandle,
    supervisor: Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: SharedAudioStatus,
    session_id: SessionId,
    cancellation: CancellationToken,
    events: crossbeam_channel::Receiver<CaptureEvent>,
    audio_sender: crossbeam_channel::Sender<AudioFrame>,
    overflow_audio_receiver: crossbeam_channel::Receiver<AudioFrame>,
}

impl ActiveAudioResources {
    fn stop(mut self) -> Result<(), ApplicationError> {
        let capture_error = self.capture.stop().err().map(|error| error.to_string());
        let event_error = self
            .event_forwarder
            .take()
            .and_then(|worker| worker.join().err())
            .map(|panic| {
                format!(
                    "Capture event worker panicked: {}",
                    crate::panic_message(panic)
                )
            });
        let transcription_error = self
            .transcription_worker
            .take()
            .and_then(|worker| worker.join().err())
            .map(|panic| {
                format!(
                    "Transcription worker panicked: {}",
                    crate::panic_message(panic)
                )
            });

        capture_error
            .or(event_error)
            .or(transcription_error)
            .map_or(Ok(()), |message| {
                Err(application_error(
                    ApplicationErrorCode::CaptureFailed,
                    message,
                    ErrorRecoverability::Retryable,
                    RecoveryAction::StopAndRetry,
                    Some(self.session_id),
                ))
            })
    }
}

pub struct AudioRuntime {
    backend: Arc<dyn AudioCaptureBackend>,
    resources: Arc<Mutex<Option<ActiveAudioResources>>>,
    status: SharedAudioStatus,
}

impl Default for AudioRuntime {
    fn default() -> Self {
        Self {
            backend: Arc::new(prollyglot_audio_windows::WindowsAudioCaptureBackend::new()),
            resources: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(PublishedAudioStatus::default())),
        }
    }
}

enum AudioQueueResult {
    Queued { dropped: u64 },
    Disconnected,
}

fn queue_latest_audio(
    sender: &crossbeam_channel::Sender<AudioFrame>,
    overflow_receiver: &crossbeam_channel::Receiver<AudioFrame>,
    frame: AudioFrame,
) -> AudioQueueResult {
    match sender.try_send(frame) {
        Ok(()) => AudioQueueResult::Queued { dropped: 0 },
        Err(TrySendError::Full(frame)) => {
            let mut dropped = 0_u64;
            while overflow_receiver.try_recv().is_ok() {
                dropped = dropped.saturating_add(1);
            }
            match sender.try_send(frame) {
                Ok(()) => AudioQueueResult::Queued { dropped },
                Err(TrySendError::Full(_)) => AudioQueueResult::Queued {
                    dropped: dropped.saturating_add(1),
                },
                Err(TrySendError::Disconnected(_)) => AudioQueueResult::Disconnected,
            }
        }
        Err(TrySendError::Disconnected(_)) => AudioQueueResult::Disconnected,
    }
}

#[tauri::command]
pub fn source_snapshot(state: State<'_, RuntimeState>) -> Result<SourceSnapshot, ApplicationError> {
    let snapshot = state.audio.backend.source_snapshot().map_err(|error| {
        tracing::error!(%error, backend = %state.audio.backend.capabilities().backend, "could not enumerate audio sources");
        application_error(
            ApplicationErrorCode::CaptureUnavailable,
            error.to_string(),
            ErrorRecoverability::Retryable,
            RecoveryAction::Retry,
            None,
        )
    })?;
    Ok(SourceSnapshot {
        playback_devices: snapshot
            .playback_devices
            .into_iter()
            .map(|device| PlaybackDevice {
                id: device.id.0,
                name: device.name,
                is_default: device.is_default,
            })
            .collect(),
        applications: snapshot
            .applications
            .into_iter()
            .map(|application| ApplicationSource {
                id: application.id.0,
                name: application.name,
                instance_count: application.instance_count,
                device_ids: application.device_ids.into_iter().map(|id| id.0).collect(),
            })
            .collect(),
    })
}

#[tauri::command]
pub fn capture_status(state: State<'_, RuntimeState>) -> CaptureStatus {
    state.audio.status.lock().capture.clone()
}

#[tauri::command]
pub async fn start_capture(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    selection: CaptureSelection,
    language: String,
) -> Result<(), ApplicationError> {
    let resolved = state
        .audio
        .backend
        .resolve_selection(&backend_capture_selection(&selection))
        .map_err(|error| {
            application_error(
                ApplicationErrorCode::CaptureUnavailable,
                error.to_string(),
                ErrorRecoverability::Retryable,
                RecoveryAction::ChooseAnotherSource,
                None,
            )
        })?;
    let request = StartSessionRequest {
        mode: SessionMode::AudioCaptions,
        source: session_source(&resolved),
    };
    let started = {
        let _control = state.control.lock();
        state.supervisor.lock().start(request)?
    };
    state
        .caption_presentation
        .begin_session(started.session_id, started.snapshot.revision);
    let transcript_snapshot = {
        let mut transcript = state.transcript.lock();
        transcript.clear();
        transcript.snapshot().clone()
    };
    if let Err(error) = app.emit("transcript-update", transcript_snapshot) {
        tracing::warn!(%error, "could not emit cleared transcript");
    }
    publish_runtime_snapshot(&app, &state.audio.status, started.snapshot.clone());
    spawn_session_monitor(
        app.clone(),
        Arc::clone(&state.supervisor),
        Arc::clone(&state.audio.resources),
        Arc::clone(&state.audio.status),
        state.caption_presentation.clone(),
        started.session_id,
    )
    .inspect_err(|error| {
        fail_and_publish(
            &app,
            &state.supervisor,
            &state.audio.status,
            started.session_id,
            error.clone(),
        );
        let _ = state
            .supervisor
            .lock()
            .finish_cleanup(started.session_id, Ok(()));
    })?;

    let mut startup_reporter = Some(state.supervisor.lock().register_worker(
        started.session_id,
        WorkerRole::ModelPreparation,
        WorkerLifetime::Startup,
    )?);
    tracing::info!(?selection, %language, session_id = %started.session_id, "starting caption session");

    let model_id = match models_for_start(&state, &language, started.session_id) {
        Ok(model_id) => model_id,
        Err(error) => {
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };
    if let Ok(snapshot) = state.supervisor.lock().update_start_progress(
        started.session_id,
        SessionProgress::PreparingModel,
        Some(format!("Loading {model_id}…")),
    ) {
        publish_runtime_snapshot(&app, &state.audio.status, snapshot);
    }

    tracing::info!(%model_id, %language, session_id = %started.session_id, "loading selected speech model");
    let model_load_started = Instant::now();
    let model_root = match prollyglot_models_root(&app, started.session_id) {
        Ok(root) => root,
        Err(error) => {
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };
    let model_id_for_worker = model_id.clone();
    let language_for_worker = language.clone();
    let prepared = match tauri::async_runtime::spawn_blocking(move || {
        crate::transcription::prepare_stream(model_root, &model_id_for_worker, language_for_worker)
    })
    .await
    {
        Ok(Ok(prepared)) => prepared,
        Ok(Err(message)) => {
            let error = application_error(
                ApplicationErrorCode::ModelFailed,
                message,
                ErrorRecoverability::UserActionRequired,
                RecoveryAction::InstallModel,
                Some(started.session_id),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
        Err(join_error) => {
            let error = application_error(
                ApplicationErrorCode::WorkerPanicked,
                format!("Could not join the model-loading worker: {join_error}"),
                ErrorRecoverability::Retryable,
                RecoveryAction::StopAndRetry,
                Some(started.session_id),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Panicked);
            return Err(error);
        }
    };
    tracing::info!(
        %model_id,
        %language,
        session_id = %started.session_id,
        elapsed_ms = model_load_started.elapsed().as_millis(),
        "selected speech model ready"
    );

    if started.cancellation.is_cancelled() {
        finish_reporter(&mut startup_reporter, WorkerOutcome::Cancelled);
        return Err(startup_cancelled(started.session_id));
    }

    if let Ok(snapshot) = state.supervisor.lock().update_start_progress(
        started.session_id,
        SessionProgress::StartingCapture,
        None,
    ) {
        publish_runtime_snapshot(&app, &state.audio.status, snapshot);
    }

    let (event_sender, event_receiver) = crossbeam_channel::bounded(12);
    let capture = match state
        .audio
        .backend
        .start_capture(resolved.selection.clone(), event_sender)
    {
        Ok(capture) => capture,
        Err(error) => {
            let error = application_error(
                ApplicationErrorCode::CaptureFailed,
                error.to_string(),
                ErrorRecoverability::Retryable,
                RecoveryAction::ChooseAnotherSource,
                Some(started.session_id),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };

    let (audio_sender, audio_receiver) = crossbeam_channel::bounded(INFERENCE_QUEUE_CAPACITY);
    let overflow_audio_receiver = audio_receiver.clone();
    let transcription_reporter = state.supervisor.lock().register_worker(
        started.session_id,
        WorkerRole::Transcription,
        WorkerLifetime::Session,
    )?;
    let app_for_transcription = app.clone();
    let transcript = Arc::clone(&state.transcript);
    let transcription_cancellation = started.cancellation.clone();
    let transcription_worker = match thread::Builder::new()
        .name("streaming-transcription".into())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::transcription::run(
                    app_for_transcription,
                    audio_receiver,
                    prepared,
                    transcript,
                )
            }));
            let outcome = match outcome {
                Ok(Ok(())) if transcription_cancellation.is_cancelled() => WorkerOutcome::Cancelled,
                Ok(Ok(())) => WorkerOutcome::Completed,
                Ok(Err(message)) => WorkerOutcome::Failed(application_error(
                    ApplicationErrorCode::ModelFailed,
                    message,
                    ErrorRecoverability::Retryable,
                    RecoveryAction::StopAndRetry,
                    Some(started.session_id),
                )),
                Err(_) => WorkerOutcome::Panicked,
            };
            transcription_reporter.finish(outcome);
        }) {
        Ok(worker) => worker,
        Err(error) => {
            let error = application_error(
                ApplicationErrorCode::WorkerExited,
                format!("Could not start the transcription worker: {error}"),
                ErrorRecoverability::Retryable,
                RecoveryAction::StopAndRetry,
                Some(started.session_id),
            );
            let resources = ActiveAudioResources {
                session_id: started.session_id,
                capture,
                event_forwarder: None,
                transcription_worker: None,
            };
            schedule_cleanup(
                &app,
                &state.supervisor,
                &state.audio.status,
                started.session_id,
                Some(resources),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };

    let event_reporter = state.supervisor.lock().register_worker(
        started.session_id,
        WorkerRole::CaptureEvents,
        WorkerLifetime::Session,
    )?;
    let forwarder = match spawn_event_forwarder(
        AudioEventForwarder {
            app: app.clone(),
            supervisor: Arc::clone(&state.supervisor),
            status: Arc::clone(&state.audio.status),
            session_id: started.session_id,
            cancellation: started.cancellation.clone(),
            events: event_receiver,
            audio_sender,
            overflow_audio_receiver,
        },
        event_reporter,
    ) {
        Ok(forwarder) => forwarder,
        Err(error) => {
            let resources = ActiveAudioResources {
                session_id: started.session_id,
                capture,
                event_forwarder: None,
                transcription_worker: Some(transcription_worker),
            };
            schedule_cleanup(
                &app,
                &state.supervisor,
                &state.audio.status,
                started.session_id,
                Some(resources),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };

    let resources = ActiveAudioResources {
        session_id: started.session_id,
        capture,
        event_forwarder: Some(forwarder),
        transcription_worker: Some(transcription_worker),
    };
    let resources = {
        let _control = state.control.lock();
        if started.cancellation.is_cancelled() {
            Some(resources)
        } else {
            let mut slot = state.audio.resources.lock();
            if slot.is_some() {
                Some(resources)
            } else {
                *slot = Some(resources);
                None
            }
        }
    };
    if let Some(resources) = resources {
        schedule_cleanup(
            &app,
            &state.supervisor,
            &state.audio.status,
            started.session_id,
            Some(resources),
        );
        finish_reporter(&mut startup_reporter, WorkerOutcome::Cancelled);
        return Err(startup_cancelled(started.session_id));
    }

    finish_reporter(&mut startup_reporter, WorkerOutcome::Completed);
    let lifecycle = state.supervisor.lock().snapshot().lifecycle;
    if lifecycle == SessionLifecycle::Starting {
        let running = match state.supervisor.lock().mark_running(started.session_id) {
            Ok(snapshot) => snapshot,
            Err(error) if error.code == ApplicationErrorCode::StartupCancelled => {
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        publish_runtime_snapshot(&app, &state.audio.status, running);
    } else if lifecycle != SessionLifecycle::Waiting {
        return Err(startup_cancelled(started.session_id));
    }
    let overlay_result = {
        let _control = state.control.lock();
        if started.cancellation.is_cancelled() {
            return Err(startup_cancelled(started.session_id));
        }
        if state.supervisor.lock().snapshot().lifecycle == SessionLifecycle::Running {
            show_live_overlay(&app, &state)
        } else {
            Ok(())
        }
    };
    if let Err(message) = overlay_result {
        tracing::warn!(%message, "could not show live caption overlay");
        if let Ok(snapshot) = state.supervisor.lock().update_health(
            started.session_id,
            SessionHealthLevel::Degraded,
            Some(message),
        ) {
            publish_runtime_snapshot(&app, &state.audio.status, snapshot);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn stop_capture(
    app: AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<(), ApplicationError> {
    let (permit, resources) = {
        let _control = state.control.lock();
        let permit = state
            .supervisor
            .lock()
            .request_stop_for_mode(SessionMode::AudioCaptions)?;
        let resources = take_resources(&state.audio.resources, permit.session_id);
        (permit, resources)
    };
    let runtime_revision = permit.snapshot.revision;
    publish_runtime_snapshot(&app, &state.audio.status, permit.snapshot);
    state
        .caption_presentation
        .clear_and_hide(&app, permit.session_id, runtime_revision);
    if !permit.already_stopping || resources.is_some() {
        schedule_cleanup(
            &app,
            &state.supervisor,
            &state.audio.status,
            permit.session_id,
            resources,
        );
    }
    Ok(())
}

pub fn is_active(state: &RuntimeState) -> bool {
    let supervisor = state.supervisor.lock();
    supervisor.has_active_session()
        && supervisor.snapshot().mode == Some(SessionMode::AudioCaptions)
}

pub fn is_live(state: &RuntimeState) -> bool {
    let snapshot = state.supervisor.lock().snapshot();
    snapshot.mode == Some(SessionMode::AudioCaptions)
        && matches!(
            snapshot.lifecycle,
            SessionLifecycle::Starting | SessionLifecycle::Running | SessionLifecycle::Waiting
        )
}

fn models_for_start(
    state: &RuntimeState,
    language: &str,
    session_id: SessionId,
) -> Result<String, ApplicationError> {
    crate::models::selected_model_id(&state.model, language).map_err(|message| {
        application_error(
            ApplicationErrorCode::ModelUnavailable,
            message,
            ErrorRecoverability::UserActionRequired,
            RecoveryAction::InstallModel,
            Some(session_id),
        )
    })
}

fn prollyglot_models_root(
    app: &AppHandle,
    session_id: SessionId,
) -> Result<std::path::PathBuf, ApplicationError> {
    crate::models::models_root(app).map_err(|message| {
        application_error(
            ApplicationErrorCode::ModelUnavailable,
            message,
            ErrorRecoverability::UserActionRequired,
            RecoveryAction::OpenSettings,
            Some(session_id),
        )
    })
}

fn spawn_event_forwarder(
    forwarder: AudioEventForwarder,
    reporter: WorkerReporter,
) -> Result<JoinHandle<()>, ApplicationError> {
    let session_id = forwarder.session_id;
    thread::Builder::new()
        .name("capture-event-forwarder".into())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                forward_capture_events(&forwarder)
            }));
            let outcome = match outcome {
                Ok(Ok(())) if forwarder.cancellation.is_cancelled() => WorkerOutcome::Cancelled,
                Ok(Ok(())) => WorkerOutcome::Completed,
                Ok(Err(error)) => WorkerOutcome::Failed(error),
                Err(_) => WorkerOutcome::Panicked,
            };
            reporter.finish(outcome);
        })
        .map_err(|error| {
            application_error(
                ApplicationErrorCode::WorkerExited,
                format!("Could not start the capture event worker: {error}"),
                ErrorRecoverability::Retryable,
                RecoveryAction::StopAndRetry,
                Some(session_id),
            )
        })
}

fn forward_capture_events(forwarder: &AudioEventForwarder) -> Result<(), ApplicationError> {
    let AudioEventForwarder {
        app,
        supervisor,
        status,
        session_id,
        cancellation,
        events,
        audio_sender,
        overflow_audio_receiver,
    } = forwarder;
    let session_id = *session_id;
    let mut last_peak_publish = None::<Instant>;
    let mut last_drop_publish = None::<Instant>;
    let mut inference_backlog_started = None::<Instant>;
    let mut last_inference_drop = None::<Instant>;
    let mut capture_dropped = 0_u64;
    let mut inference_dropped = 0_u64;
    loop {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        let event = match events.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };
        match event {
            CaptureEvent::State(BackendCaptureState::Starting) => {}
            CaptureEvent::State(BackendCaptureState::Capturing) => {
                resume_if_needed(app, supervisor, status, session_id);
            }
            CaptureEvent::State(BackendCaptureState::Waiting) => {
                mark_waiting(
                    app,
                    supervisor,
                    status,
                    session_id,
                    "The selected audio source is temporarily unavailable.",
                );
            }
            CaptureEvent::State(BackendCaptureState::Stopping | BackendCaptureState::Stopped) => {
                if !cancellation.is_cancelled() {
                    return Err(application_error(
                        ApplicationErrorCode::CaptureFailed,
                        "The audio capture session stopped unexpectedly.",
                        ErrorRecoverability::Retryable,
                        RecoveryAction::StopAndRetry,
                        Some(session_id),
                    ));
                }
                return Ok(());
            }
            CaptureEvent::State(BackendCaptureState::Failed) => {
                return Err(application_error(
                    ApplicationErrorCode::CaptureFailed,
                    "The audio capture worker reported a failure.",
                    ErrorRecoverability::Retryable,
                    RecoveryAction::ChooseAnotherSource,
                    Some(session_id),
                ));
            }
            CaptureEvent::Frame(frame) => {
                resume_if_needed(app, supervisor, status, session_id);
                let peak = frame.peak;
                let dropped_now = match queue_latest_audio(
                    audio_sender,
                    overflow_audio_receiver,
                    frame,
                ) {
                    AudioQueueResult::Queued { dropped } => {
                        if dropped > 0 {
                            let now = Instant::now();
                            inference_dropped = inference_dropped.saturating_add(dropped);
                            inference_backlog_started.get_or_insert(now);
                            last_inference_drop = Some(now);
                            if last_drop_publish
                                .is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
                            {
                                let message = format!(
                                    "Transcription fell behind; {inference_dropped} old audio packets were skipped to stay live."
                                );
                                tracing::warn!(
                                    dropped_packets = dropped,
                                    total_dropped_packets = inference_dropped,
                                    queue_capacity = INFERENCE_QUEUE_CAPACITY,
                                    "transcription inference queue fell behind"
                                );
                                update_runtime_health(
                                    app,
                                    supervisor,
                                    status,
                                    session_id,
                                    SessionHealthLevel::Degraded,
                                    Some(message),
                                );
                                last_drop_publish = Some(now);
                            }
                        }
                        dropped
                    }
                    AudioQueueResult::Disconnected => {
                        return Err(application_error(
                            ApplicationErrorCode::WorkerExited,
                            "The transcription worker stopped accepting audio.",
                            ErrorRecoverability::Retryable,
                            RecoveryAction::StopAndRetry,
                            Some(session_id),
                        ));
                    }
                };

                if dropped_now == 0
                    && audio_sender.len() <= INFERENCE_QUEUE_RECOVERY_DEPTH
                    && last_inference_drop
                        .is_some_and(|last| last.elapsed() >= INFERENCE_BACKLOG_RECOVERY_GRACE)
                {
                    tracing::info!(
                        total_dropped_packets = inference_dropped,
                        backlog_millis = inference_backlog_started
                            .map_or(0, |started| started.elapsed().as_millis()),
                        queue_depth = audio_sender.len(),
                        "transcription inference queue recovered"
                    );
                    inference_backlog_started = None;
                    last_inference_drop = None;
                    last_drop_publish = None;
                    update_runtime_health(
                        app,
                        supervisor,
                        status,
                        session_id,
                        SessionHealthLevel::Healthy,
                        None,
                    );
                }

                if last_peak_publish.is_some_and(|last| last.elapsed() < Duration::from_millis(50))
                {
                    continue;
                }
                last_peak_publish = Some(Instant::now());
                publish_metrics(
                    app,
                    status,
                    supervisor,
                    peak,
                    capture_dropped.saturating_add(inference_dropped),
                );
            }
            CaptureEvent::Recovery(recovery) => {
                tracing::warn!(
                    ?recovery.kind,
                    retry_after_millis = recovery.retry_after_millis,
                    "capture is waiting for its selected source"
                );
                mark_waiting(app, supervisor, status, session_id, recovery.message);
            }
            CaptureEvent::FramesDropped { total } => {
                capture_dropped = total;
                let message =
                    format!("Audio processing fell behind; {total} packets were dropped.");
                tracing::warn!(total, "audio frames dropped because the pipeline was full");
                update_runtime_health(
                    app,
                    supervisor,
                    status,
                    session_id,
                    SessionHealthLevel::Degraded,
                    Some(message),
                );
                publish_metrics(
                    app,
                    status,
                    supervisor,
                    0.0,
                    capture_dropped.saturating_add(inference_dropped),
                );
            }
            CaptureEvent::Error(message) => {
                tracing::error!(%message, "capture worker failed");
                return Err(application_error(
                    ApplicationErrorCode::CaptureFailed,
                    message,
                    ErrorRecoverability::Retryable,
                    RecoveryAction::ChooseAnotherSource,
                    Some(session_id),
                ));
            }
        }
    }
}

fn spawn_session_monitor(
    app: AppHandle,
    supervisor: Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    resources: Arc<Mutex<Option<ActiveAudioResources>>>,
    status: SharedAudioStatus,
    caption_presentation: crate::presentation::CaptionPresentationRuntime,
    session_id: SessionId,
) -> Result<(), ApplicationError> {
    thread::Builder::new()
        .name(format!("session-supervisor-{session_id}"))
        .spawn(move || {
            let mut stopping_since = None::<Instant>;
            let mut failure_cleanup_started = false;
            loop {
                let updates = supervisor.lock().drain_worker_completions();
                for update in updates {
                    publish_runtime_snapshot(&app, &status, update);
                }
                let (snapshot, active) = {
                    let supervisor = supervisor.lock();
                    (supervisor.snapshot(), supervisor.has_active_session())
                };
                if !active || snapshot.session_id != Some(session_id) {
                    break;
                }
                if snapshot.lifecycle == SessionLifecycle::Stopping {
                    let started = stopping_since.get_or_insert_with(Instant::now);
                    if started.elapsed() >= SHUTDOWN_TIMEOUT {
                        let timed_out = supervisor.lock().shutdown_timed_out(session_id);
                        if let Ok(snapshot) = timed_out {
                            publish_runtime_snapshot(&app, &status, snapshot);
                        }
                    }
                } else {
                    stopping_since = None;
                }
                if snapshot.lifecycle == SessionLifecycle::Failed && !failure_cleanup_started {
                    failure_cleanup_started = true;
                    caption_presentation.clear_and_hide(&app, session_id, snapshot.revision);
                    let resources = take_resources(&resources, session_id);
                    schedule_cleanup(&app, &supervisor, &status, session_id, resources);
                }
                thread::sleep(SUPERVISOR_POLL_INTERVAL);
            }
        })
        .map(|_| ())
        .map_err(|error| {
            application_error(
                ApplicationErrorCode::WorkerExited,
                format!("Could not start the session supervisor: {error}"),
                ErrorRecoverability::RestartRequired,
                RecoveryAction::RestartApplication,
                Some(session_id),
            )
        })
}

fn schedule_cleanup(
    app: &AppHandle,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: &SharedAudioStatus,
    session_id: SessionId,
    resources: Option<ActiveAudioResources>,
) {
    let Some(resources) = resources else {
        if let Ok(Some(snapshot)) = supervisor.lock().finish_cleanup(session_id, Ok(())) {
            publish_runtime_snapshot(app, status, snapshot);
        }
        return;
    };
    let reporter = match supervisor.lock().register_worker(
        session_id,
        WorkerRole::Shutdown,
        WorkerLifetime::Shutdown,
    ) {
        Ok(reporter) => reporter,
        Err(error) => {
            tracing::error!(%error, session_id = %session_id, "could not supervise audio cleanup");
            let _ = thread::Builder::new()
                .name("audio-cleanup-untracked".into())
                .spawn(move || {
                    let _ = resources.stop();
                });
            return;
        }
    };
    let app_for_worker = app.clone();
    let supervisor_for_worker = Arc::clone(supervisor);
    let status_for_worker = Arc::clone(status);
    let spawn = thread::Builder::new()
        .name("audio-session-stop".into())
        .spawn(move || {
            let result = resources.stop();
            reporter.finish(WorkerOutcome::Completed);
            match supervisor_for_worker
                .lock()
                .finish_cleanup(session_id, result)
            {
                Ok(Some(snapshot)) => {
                    publish_runtime_snapshot(&app_for_worker, &status_for_worker, snapshot);
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, session_id = %session_id, "audio cleanup completed after the session changed"),
            }
        });
    if let Err(error) = spawn {
        let failure = application_error(
            ApplicationErrorCode::WorkerExited,
            format!("Could not start the audio cleanup worker: {error}"),
            ErrorRecoverability::RestartRequired,
            RecoveryAction::RestartApplication,
            Some(session_id),
        );
        fail_and_publish(app, supervisor, status, session_id, failure.clone());
        if let Ok(Some(snapshot)) = supervisor.lock().finish_cleanup(session_id, Err(failure)) {
            publish_runtime_snapshot(app, status, snapshot);
        }
    }
}

fn take_resources(
    resources: &Arc<Mutex<Option<ActiveAudioResources>>>,
    session_id: SessionId,
) -> Option<ActiveAudioResources> {
    let mut resources = resources.lock();
    if resources
        .as_ref()
        .is_some_and(|resources| resources.session_id == session_id)
    {
        resources.take()
    } else {
        None
    }
}

fn resume_if_needed(
    app: &AppHandle,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: &SharedAudioStatus,
    session_id: SessionId,
) {
    let lifecycle = supervisor.lock().snapshot().lifecycle;
    if lifecycle == SessionLifecycle::Waiting
        && let Ok(snapshot) = supervisor.lock().mark_running(session_id)
    {
        publish_runtime_snapshot(app, status, snapshot);
    }
}

fn mark_waiting(
    app: &AppHandle,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: &SharedAudioStatus,
    session_id: SessionId,
    message: impl Into<String>,
) {
    if let Ok(snapshot) = supervisor.lock().mark_waiting(session_id, message) {
        publish_runtime_snapshot(app, status, snapshot);
    }
}

fn update_runtime_health(
    app: &AppHandle,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: &SharedAudioStatus,
    session_id: SessionId,
    level: SessionHealthLevel,
    message: Option<String>,
) {
    if let Ok(snapshot) = supervisor.lock().update_health(session_id, level, message) {
        publish_runtime_snapshot(app, status, snapshot);
    }
}

fn publish_runtime_snapshot(
    app: &AppHandle,
    status: &SharedAudioStatus,
    snapshot: RuntimeSnapshot,
) {
    if !crate::runtime::publish_snapshot(app, &snapshot) {
        return;
    }
    let mut published = status.lock();
    let published_revision = published.runtime_revision;
    let Some(next) = published.apply_snapshot(&snapshot) else {
        tracing::debug!(
            revision = snapshot.revision,
            published_revision,
            "ignored an out-of-order runtime snapshot"
        );
        return;
    };
    emit_capture_status(app, next);
}

fn projected_status(current: &CaptureStatus, snapshot: &RuntimeSnapshot) -> CaptureStatus {
    let state = match snapshot.lifecycle {
        SessionLifecycle::Stopped => CaptureState::Stopped,
        SessionLifecycle::Starting => CaptureState::Starting,
        SessionLifecycle::Running => CaptureState::Capturing,
        SessionLifecycle::Waiting => CaptureState::Waiting,
        SessionLifecycle::Stopping => CaptureState::Stopping,
        SessionLifecycle::Failed => CaptureState::Failed,
    };
    let stopped = state == CaptureState::Stopped;
    CaptureStatus {
        state,
        peak: if matches!(state, CaptureState::Capturing | CaptureState::Waiting) {
            current.peak
        } else {
            0.0
        },
        dropped_frames: if stopped { 0 } else { current.dropped_frames },
        source_label: if stopped {
            None
        } else {
            snapshot.source.as_ref().map(|source| source.label.clone())
        },
        message: snapshot
            .failure
            .as_ref()
            .map(|failure| failure.message.clone())
            .or_else(|| snapshot.health.message.clone()),
    }
}

fn publish_metrics(
    app: &AppHandle,
    status: &SharedAudioStatus,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    peak: f32,
    dropped_frames: u64,
) {
    let snapshot = supervisor.lock().snapshot();
    let mut published = status.lock();
    if snapshot.revision != published.runtime_revision {
        return;
    }
    let mut next = projected_status(&published.capture, &snapshot);
    next.peak = peak;
    next.dropped_frames = dropped_frames;
    published.capture = next.clone();
    emit_capture_status(app, next);
}

fn emit_capture_status(app: &AppHandle, next: CaptureStatus) {
    if let Err(error) = app.emit(
        prollyglot_application_runtime::ipc::CAPTURE_STATUS_EVENT,
        next,
    ) {
        tracing::warn!(%error, "could not emit capture status");
    }
}

fn fail_and_publish(
    app: &AppHandle,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: &SharedAudioStatus,
    session_id: SessionId,
    error: ApplicationError,
) {
    tracing::error!(%error, session_id = %session_id, "caption session failed");
    if let Ok(snapshot) = supervisor.lock().fail(session_id, error) {
        publish_runtime_snapshot(app, status, snapshot);
    }
}

fn session_source(selection: &ResolvedCaptureSelection) -> SessionSource {
    let kind = match &selection.selection {
        BackendCaptureSelection::SystemDefault | BackendCaptureSelection::SystemOutput { .. } => {
            SessionSourceKind::SystemOutput
        }
        BackendCaptureSelection::Application { .. } => SessionSourceKind::Application,
    };
    SessionSource::new(
        selection.source_id.0.clone(),
        kind,
        selection.display_name.clone(),
    )
}

fn backend_capture_selection(selection: &CaptureSelection) -> BackendCaptureSelection {
    match selection {
        CaptureSelection::SystemDefault => BackendCaptureSelection::SystemDefault,
        CaptureSelection::SystemOutput { device_id } => BackendCaptureSelection::SystemOutput {
            device_id: prollyglot_core::SourceId::new(device_id.clone()),
        },
        CaptureSelection::Application { source_id } => BackendCaptureSelection::Application {
            source_id: prollyglot_core::SourceId::new(source_id.clone()),
        },
    }
}

fn application_error(
    code: ApplicationErrorCode,
    message: impl Into<String>,
    recoverability: ErrorRecoverability,
    action: RecoveryAction,
    session_id: Option<SessionId>,
) -> ApplicationError {
    let error = ApplicationError::new(code, message, recoverability, action);
    match session_id {
        Some(session_id) => error.for_session(session_id),
        None => error,
    }
}

fn startup_cancelled(session_id: SessionId) -> ApplicationError {
    application_error(
        ApplicationErrorCode::StartupCancelled,
        "Caption startup was cancelled.",
        ErrorRecoverability::Retryable,
        RecoveryAction::Retry,
        Some(session_id),
    )
}

fn finish_reporter(reporter: &mut Option<WorkerReporter>, outcome: WorkerOutcome) {
    if let Some(reporter) = reporter.take() {
        reporter.finish(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_inference_queue_discards_all_stale_packets() {
        let (sender, receiver) = crossbeam_channel::bounded(2);
        let overflow_receiver = receiver.clone();
        let frame = |sequence| AudioFrame {
            sequence,
            source_id: prollyglot_core::SourceId::new("test"),
            captured_at_micros: sequence,
            sample_rate: 16_000,
            samples: vec![0.0],
            peak: 0.0,
            discontinuity: false,
        };
        sender.try_send(frame(0)).expect("first packet");
        sender.try_send(frame(1)).expect("second packet");

        assert!(matches!(
            queue_latest_audio(&sender, &overflow_receiver, frame(2)),
            AudioQueueResult::Queued { dropped: 2 }
        ));
        assert_eq!(receiver.recv().expect("newest").sequence, 2);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn capture_status_is_only_a_projection_of_runtime_lifecycle() {
        let current = CaptureStatus {
            state: CaptureState::Capturing,
            peak: 0.7,
            dropped_frames: 4,
            source_label: Some("old".into()),
            message: Some("old".into()),
        };
        let snapshot = RuntimeSnapshot {
            session_id: Some(SessionId(9)),
            mode: Some(SessionMode::AudioCaptions),
            source: Some(SessionSource::new(
                "default-output",
                SessionSourceKind::SystemOutput,
                "System default",
            )),
            lifecycle: SessionLifecycle::Waiting,
            health: prollyglot_application_runtime::RuntimeHealth::recovering("Waiting for audio"),
            ..RuntimeSnapshot::default()
        };

        let projected = projected_status(&current, &snapshot);

        assert_eq!(projected.state, CaptureState::Waiting);
        assert_eq!(projected.source_label.as_deref(), Some("System default"));
        assert_eq!(projected.message.as_deref(), Some("Waiting for audio"));
        assert_eq!(projected.peak, 0.7);
    }

    #[test]
    fn out_of_order_runtime_snapshots_cannot_regress_the_compatibility_status() {
        let mut published = PublishedAudioStatus::default();
        let running = RuntimeSnapshot {
            revision: 4,
            session_id: Some(SessionId(2)),
            mode: Some(SessionMode::AudioCaptions),
            lifecycle: SessionLifecycle::Running,
            ..RuntimeSnapshot::default()
        };
        let stale_starting = RuntimeSnapshot {
            revision: 3,
            lifecycle: SessionLifecycle::Starting,
            ..running.clone()
        };

        assert!(published.apply_snapshot(&running).is_some());
        assert!(published.apply_snapshot(&stale_starting).is_none());
        assert_eq!(published.runtime_revision, 4);
        assert_eq!(published.capture.state, CaptureState::Capturing);
    }
}
