mod models;
mod transcription;

use std::{
    fs,
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{RecvTimeoutError, TrySendError};
use parking_lot::Mutex;
use prollyglot_core::{
    AudioFrame, CaptureEvent, CaptureSelection, CaptureSession, CaptureState, SourceSnapshot,
};
use prollyglot_transcript::{TranscriptSnapshot, TranscriptStore};
use serde::{Deserialize, Serialize};
use tauri::{
    Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalRect, PhysicalSize, State,
    WebviewWindow,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureStatus {
    state: CaptureState,
    peak: f32,
    dropped_frames: u64,
    source_label: Option<String>,
    message: Option<String>,
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

struct ActiveSession {
    capture: Box<dyn CaptureSession>,
    event_forwarder: Option<JoinHandle<()>>,
    transcription_worker: Option<JoinHandle<()>>,
}

enum AudioQueueResult {
    Queued { dropped: u64 },
    Disconnected,
}

const INFERENCE_QUEUE_CAPACITY: usize = 128;
const INFERENCE_QUEUE_RECOVERY_DEPTH: usize = INFERENCE_QUEUE_CAPACITY / 4;
const INFERENCE_BACKLOG_MESSAGE_PREFIX: &str = "Transcription fell behind;";
const INFERENCE_BACKLOG_RECOVERY_GRACE: Duration = Duration::from_secs(2);

fn queue_latest_audio(
    sender: &crossbeam_channel::Sender<AudioFrame>,
    overflow_receiver: &crossbeam_channel::Receiver<AudioFrame>,
    frame: AudioFrame,
) -> AudioQueueResult {
    match sender.try_send(frame) {
        Ok(()) => AudioQueueResult::Queued { dropped: 0 },
        Err(TrySendError::Full(frame)) => {
            // A full queue means transcription is already processing stale
            // audio. Empty it in one step so the worker can resume near the
            // live edge instead of dropping a single packet on every capture
            // callback and repeatedly resetting on sequence gaps.
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

impl ActiveSession {
    fn stop(&mut self) -> Result<(), String> {
        let capture_error = self.capture.stop().err().map(|error| error.to_string());
        let event_error = self
            .event_forwarder
            .take()
            .and_then(|worker| worker.join().err())
            .map(|panic| format!("capture event worker panicked: {}", panic_message(panic)));
        let transcription_error = self
            .transcription_worker
            .take()
            .and_then(|worker| worker.join().err())
            .map(|panic| format!("transcription worker panicked: {}", panic_message(panic)));

        capture_error
            .or(event_error)
            .or(transcription_error)
            .map_or(Ok(()), Err)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlaySettings {
    font_family: String,
    font_size: u16,
    text_color: String,
    translated_text_color: String,
    bilingual_layout: BilingualLayout,
    background_opacity: f32,
    width: u32,
    maximum_lines: u8,
    reading_time_seconds: u16,
    fade_duration_ms: u16,
    position: OverlayPosition,
    click_through: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum OverlayPosition {
    TopCenter,
    BottomCenter,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum BilingualLayout {
    Stacked,
    SideBySide,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            font_family: r#""Segoe UI Variable", "Segoe UI", sans-serif"#.into(),
            font_size: 36,
            text_color: "#f4f6f5".into(),
            translated_text_color: "#86e3b0".into(),
            bilingual_layout: BilingualLayout::Stacked,
            background_opacity: 0.75,
            width: 720,
            maximum_lines: 3,
            reading_time_seconds: 15,
            fade_duration_ms: 800,
            position: OverlayPosition::BottomCenter,
            click_through: true,
        }
    }
}

#[derive(Default)]
struct RuntimeState {
    control: Mutex<()>,
    session: Mutex<Option<ActiveSession>>,
    status: Arc<Mutex<CaptureStatus>>,
    transcript: Arc<Mutex<TranscriptStore>>,
    model: models::ModelRuntime,
    overlay_settings: Mutex<OverlaySettings>,
}

struct LoggingGuard {
    _worker: tracing_appender::non_blocking::WorkerGuard,
}

fn initialize_logging(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let log_directory = app.path().app_log_dir()?;
    fs::create_dir_all(&log_directory)?;
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("prollyglot")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_directory)?;
    let (writer, worker) = tracing_appender::non_blocking(file_appender);
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(true)
        .with_max_level(tracing::Level::INFO)
        .compact()
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    app.manage(LoggingGuard { _worker: worker });
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Prollyglot started");
    Ok(())
}

fn publish_status(app: &tauri::AppHandle, status: &Arc<Mutex<CaptureStatus>>, next: CaptureStatus) {
    *status.lock() = next.clone();
    if let Err(error) = app.emit("capture-status", next) {
        tracing::warn!(%error, "could not emit capture status");
    }
}

fn publish_capture_failure(
    app: &tauri::AppHandle,
    status: &Arc<Mutex<CaptureStatus>>,
    source_label: Option<String>,
    message: String,
) -> String {
    tracing::error!(%message, "could not start captions");
    publish_status(
        app,
        status,
        CaptureStatus {
            state: CaptureState::Failed,
            peak: 0.0,
            dropped_frames: 0,
            source_label,
            message: Some(message.clone()),
        },
    );
    message
}

#[tauri::command]
fn source_snapshot() -> Result<SourceSnapshot, String> {
    prollyglot_audio_windows::source_snapshot().map_err(|error| {
        tracing::error!(%error, "could not enumerate Windows audio sources");
        error.to_string()
    })
}

#[tauri::command]
fn capture_status(state: State<'_, RuntimeState>) -> CaptureStatus {
    state.status.lock().clone()
}

#[tauri::command]
async fn start_capture(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
    selection: CaptureSelection,
    language: String,
) -> Result<(), String> {
    let source_label = Some(selection.source_id().to_string());
    let mut stale_session = {
        let _control = state.control.lock();
        if matches!(
            state.status.lock().state,
            CaptureState::Starting
                | CaptureState::Capturing
                | CaptureState::Waiting
                | CaptureState::Stopping
        ) {
            return Err("A caption session is already starting or running.".into());
        }
        let stale_session = state.session.lock().take();
        publish_status(
            &app,
            &state.status,
            CaptureStatus {
                state: CaptureState::Starting,
                peak: 0.0,
                dropped_frames: 0,
                source_label: source_label.clone(),
                message: None,
            },
        );
        stale_session
    };
    if let Some(session) = stale_session.as_mut()
        && let Err(error) = session.stop()
    {
        tracing::warn!(%error, "previous caption session did not clean up normally");
    }
    drop(stale_session);

    tracing::info!(?selection, %language, "starting caption session");
    let model_id = models::selected_model_id(&state.model, &language).map_err(|message| {
        publish_capture_failure(&app, &state.status, source_label.clone(), message)
    })?;
    let model_root = models::models_root(&app).map_err(|message| {
        publish_capture_failure(&app, &state.status, source_label.clone(), message)
    })?;
    tracing::info!(%model_id, %language, "loading selected speech model");
    let model_load_started = Instant::now();
    let model_id_for_worker = model_id.clone();
    let language_for_worker = language.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        transcription::prepare_stream(model_root, &model_id_for_worker, language_for_worker)
    })
    .await
    .map_err(|error| {
        publish_capture_failure(
            &app,
            &state.status,
            source_label.clone(),
            format!("Could not join the model-loading worker: {error}"),
        )
    })?
    .map_err(|message| {
        publish_capture_failure(&app, &state.status, source_label.clone(), message)
    })?;
    tracing::info!(
        %model_id,
        %language,
        elapsed_ms = model_load_started.elapsed().as_millis(),
        "selected speech model ready"
    );

    let transcript_snapshot = {
        let mut transcript = state.transcript.lock();
        transcript.clear();
        transcript.snapshot().clone()
    };
    if let Err(error) = app.emit("transcript-update", transcript_snapshot) {
        tracing::warn!(%error, "could not emit cleared transcript");
    }

    // Nemotron decodes in larger streaming windows than the smaller English
    // models. Keep enough raw WASAPI packets to absorb a normal inference
    // burst without turning that model latency into a discontinuity.
    let (audio_sender, audio_receiver) = crossbeam_channel::bounded(INFERENCE_QUEUE_CAPACITY);
    let overflow_audio_receiver = audio_receiver.clone();
    let (transcription_error_sender, transcription_error_receiver) = crossbeam_channel::bounded(1);
    let transcription_panic_sender = transcription_error_sender.clone();
    let app_for_transcription = app.clone();
    let transcript = Arc::clone(&state.transcript);
    let transcription_worker = thread::Builder::new()
        .name("streaming-transcription".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                transcription::run(
                    app_for_transcription,
                    audio_receiver,
                    prepared,
                    transcript,
                    transcription_error_sender,
                );
            }));
            if let Err(panic) = result {
                let message = panic
                    .downcast_ref::<&str>()
                    .map(|message| (*message).to_owned())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic payload".into());
                let _ = transcription_panic_sender
                    .try_send(format!("The transcription worker panicked: {message}"));
                std::panic::resume_unwind(panic);
            }
        })
        .map_err(|error| {
            publish_capture_failure(
                &app,
                &state.status,
                source_label.clone(),
                format!("Could not start the transcription worker: {error}"),
            )
        })?;

    let (event_sender, event_receiver) = crossbeam_channel::bounded(12);
    let mut capture = match prollyglot_audio_windows::start_capture(selection, event_sender) {
        Ok(session) => session,
        Err(error) => {
            drop(audio_sender);
            let _ = transcription_worker.join();
            return Err(publish_capture_failure(
                &app,
                &state.status,
                source_label,
                error.to_string(),
            ));
        }
    };

    let app_for_events = app.clone();
    let status_for_events = Arc::clone(&state.status);
    let forwarder = std::thread::Builder::new()
        .name("capture-event-forwarder".into())
        .spawn(move || {
            let mut last_peak_publish = None::<Instant>;
            let mut last_drop_publish = None::<Instant>;
            let mut inference_backlog_started = None::<Instant>;
            let mut last_inference_drop = None::<Instant>;
            let mut capture_dropped = 0_u64;
            let mut inference_dropped = 0_u64;
            loop {
                if let Ok(message) = transcription_error_receiver.try_recv() {
                    let previous = status_for_events.lock().clone();
                    publish_status(
                        &app_for_events,
                        &status_for_events,
                        CaptureStatus {
                            state: CaptureState::Failed,
                            peak: 0.0,
                            message: Some(message),
                            ..previous
                        },
                    );
                    break;
                }
                let event = match event_receiver.recv_timeout(Duration::from_millis(50)) {
                    Ok(event) => event,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                };

                let previous = status_for_events.lock().clone();
                let next = match event {
                    CaptureEvent::State(capture_state) => CaptureStatus {
                        state: capture_state,
                        message: if capture_state == CaptureState::Capturing {
                            None
                        } else {
                            previous.message.clone()
                        },
                        ..previous
                    },
                    CaptureEvent::Frame(frame) => {
                        let peak = frame.peak;
                        let dropped_now =
                            match queue_latest_audio(&audio_sender, &overflow_audio_receiver, frame) {
                            AudioQueueResult::Queued { dropped } => {
                                if dropped > 0 {
                                    let now = Instant::now();
                                    inference_dropped = inference_dropped.saturating_add(dropped);
                                    inference_backlog_started.get_or_insert(now);
                                    last_inference_drop = Some(now);
                                    let should_publish = last_drop_publish
                                        .is_none_or(|last| last.elapsed() >= Duration::from_secs(1));
                                    let mut current = status_for_events.lock();
                                    current.dropped_frames =
                                        capture_dropped.saturating_add(inference_dropped);
                                    if should_publish {
                                        current.message = Some(format!(
                                            "Transcription fell behind; {inference_dropped} old audio packets were skipped to stay live."
                                        ));
                                        let next = current.clone();
                                        drop(current);
                                        tracing::warn!(
                                            dropped_packets = dropped,
                                            total_dropped_packets = inference_dropped,
                                            queue_capacity = INFERENCE_QUEUE_CAPACITY,
                                            "transcription inference queue fell behind"
                                        );
                                        publish_status(&app_for_events, &status_for_events, next);
                                        last_drop_publish = Some(now);
                                    }
                                }
                                dropped
                            }
                            AudioQueueResult::Disconnected => {
                                let message = transcription_error_receiver
                                    .try_recv()
                                    .unwrap_or_else(|_| {
                                        "The transcription worker stopped unexpectedly.".into()
                                    });
                                publish_status(
                                    &app_for_events,
                                    &status_for_events,
                                    CaptureStatus {
                                        state: CaptureState::Failed,
                                        peak: 0.0,
                                        message: Some(message),
                                        ..previous
                                    },
                                );
                                break;
                            }
                        };

                        if dropped_now == 0
                            && audio_sender.len() <= INFERENCE_QUEUE_RECOVERY_DEPTH
                            && last_inference_drop.is_some_and(|last| {
                                last.elapsed() >= INFERENCE_BACKLOG_RECOVERY_GRACE
                            })
                        {
                            let backlog_millis = inference_backlog_started
                                .map_or(0, |started| started.elapsed().as_millis());
                            tracing::info!(
                                total_dropped_packets = inference_dropped,
                                backlog_millis,
                                queue_depth = audio_sender.len(),
                                "transcription inference queue recovered"
                            );
                            inference_backlog_started = None;
                            last_inference_drop = None;
                            last_drop_publish = None;
                            let mut current = status_for_events.lock();
                            if current.message.as_deref().is_some_and(|message| {
                                message.starts_with(INFERENCE_BACKLOG_MESSAGE_PREFIX)
                            }) {
                                current.message = None;
                            }
                        }

                        if last_peak_publish
                            .is_some_and(|last| last.elapsed() < Duration::from_millis(50))
                        {
                            continue;
                        }
                        last_peak_publish = Some(Instant::now());
                        let current = status_for_events.lock().clone();
                        CaptureStatus {
                            state: if current.state == CaptureState::Waiting {
                                CaptureState::Waiting
                            } else {
                                CaptureState::Capturing
                            },
                            peak,
                            dropped_frames: capture_dropped.saturating_add(inference_dropped),
                            ..current
                        }
                    }
                    CaptureEvent::Warning(message) => {
                        tracing::warn!(%message, "capture is waiting to recover");
                        CaptureStatus {
                            state: CaptureState::Waiting,
                            message: Some(message),
                            ..previous
                        }
                    }
                    CaptureEvent::FramesDropped { total } => {
                        capture_dropped = total;
                        tracing::warn!(total, "audio frames dropped because the pipeline was full");
                        CaptureStatus {
                            dropped_frames: capture_dropped.saturating_add(inference_dropped),
                            message: Some(format!(
                                "Audio processing fell behind; {total} packets were dropped."
                            )),
                            ..previous
                        }
                    }
                    CaptureEvent::Error(message) => {
                        tracing::error!(%message, "capture worker failed");
                        CaptureStatus {
                            state: CaptureState::Failed,
                            peak: 0.0,
                            message: Some(message),
                            ..previous
                        }
                    }
                };
                publish_status(&app_for_events, &status_for_events, next);
            }
        })
        .map_err(|error| format!("Could not start the capture event forwarder: {error}"));
    let forwarder = match forwarder {
        Ok(forwarder) => forwarder,
        Err(error) => {
            let _ = capture.stop();
            let _ = transcription_worker.join();
            return Err(publish_capture_failure(
                &app,
                &state.status,
                source_label,
                error,
            ));
        }
    };

    *state.session.lock() = Some(ActiveSession {
        capture,
        event_forwarder: Some(forwarder),
        transcription_worker: Some(transcription_worker),
    });
    publish_status(
        &app,
        &state.status,
        CaptureStatus {
            state: CaptureState::Capturing,
            peak: 0.0,
            dropped_frames: 0,
            source_label,
            message: None,
        },
    );
    if let Err(error) = show_live_overlay(&app, &state) {
        tracing::warn!(%error, "could not show live caption overlay");
        let previous = state.status.lock().clone();
        publish_status(
            &app,
            &state.status,
            CaptureStatus {
                message: Some(error),
                ..previous
            },
        );
    }
    Ok(())
}

#[tauri::command]
fn stop_capture(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<(), String> {
    let _control = state.control.lock();
    let Some(mut session) = state.session.lock().take() else {
        return Err("No capture session is running.".into());
    };

    let previous = state.status.lock().clone();
    publish_status(
        &app,
        &state.status,
        CaptureStatus {
            state: CaptureState::Stopping,
            peak: 0.0,
            message: None,
            ..previous.clone()
        },
    );

    if let Err(error) = session.stop() {
        tracing::error!(%error, "could not stop caption session cleanly");
        publish_status(
            &app,
            &state.status,
            CaptureStatus {
                state: CaptureState::Failed,
                peak: 0.0,
                message: Some(error.clone()),
                ..previous
            },
        );
        return Err(error);
    }
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.emit("overlay-caption", "");
        if let Err(error) = overlay.hide() {
            tracing::warn!(%error, "could not hide caption overlay");
        }
    }
    tracing::info!("caption session stopped");
    publish_status(&app, &state.status, CaptureStatus::default());
    Ok(())
}

#[tauri::command]
fn transcript_snapshot(state: State<'_, RuntimeState>) -> TranscriptSnapshot {
    state.transcript.lock().snapshot().clone()
}

#[tauri::command]
fn clear_transcript(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<(), String> {
    let snapshot = {
        let mut transcript = state.transcript.lock();
        transcript.clear();
        transcript.snapshot().clone()
    };
    app.emit("transcript-update", snapshot)
        .map_err(|error| error.to_string())?;
    app.emit("overlay-caption", "")
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn show_appearance_window(app: tauri::AppHandle) -> Result<(), String> {
    // The caption overlay is an always-on-top window. Hide it while the
    // Appearance surface is open so a large non-click-through overlay cannot
    // cover the controls and trap the user in this window.
    if let Some(overlay) = app.get_webview_window("overlay") {
        if let Err(error) = overlay.set_ignore_cursor_events(true) {
            tracing::warn!(%error, "could not make the overlay ignore input before Appearance");
        }
        if let Err(error) = overlay.hide() {
            tracing::warn!(%error, "could not hide the overlay before Appearance");
        }
    }
    let window = app
        .get_webview_window("appearance")
        .ok_or("Appearance window is unavailable.")?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
fn close_appearance_window(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<(), String> {
    // Dismiss Appearance first. Any overlay restoration failure should be
    // reported, but it must never leave the settings window trapping input.
    let appearance = app
        .get_webview_window("appearance")
        .ok_or("Appearance window is unavailable.")?;
    appearance.hide().map_err(|error| error.to_string())?;

    let captions_are_running = matches!(
        state.status.lock().state,
        CaptureState::Starting | CaptureState::Capturing | CaptureState::Waiting
    );
    if captions_are_running {
        if let Err(error) = restore_live_overlay(&app, &state) {
            tracing::warn!(%error, "Appearance closed but the live overlay could not be restored");
        }
    } else if let Some(overlay) = app.get_webview_window("overlay") {
        if let Err(error) = overlay.emit("overlay-caption", "") {
            tracing::warn!(%error, "could not clear overlay after closing Appearance");
        }
        if let Err(error) = overlay.hide() {
            tracing::warn!(%error, "could not hide overlay after closing Appearance");
        }
    }
    Ok(())
}

fn validated_settings(settings: OverlaySettings) -> Result<OverlaySettings, String> {
    if !(18..=96).contains(&settings.font_size) {
        return Err("Caption size must be between 18 and 96 pixels.".into());
    }
    if !(320..=1600).contains(&settings.width) {
        return Err("Caption width must be between 320 and 1600 pixels.".into());
    }
    if !(1..=4).contains(&settings.maximum_lines) {
        return Err("Maximum lines must be between 1 and 4.".into());
    }
    if !(3..=60).contains(&settings.reading_time_seconds) {
        return Err("Caption reading time must be between 3 and 60 seconds.".into());
    }
    if settings.fade_duration_ms > 5_000 {
        return Err("Caption fade duration must be at most 5 seconds.".into());
    }
    if !(0.0..=1.0).contains(&settings.background_opacity) {
        return Err("Background opacity must be between 0 and 1.".into());
    }
    if !is_hex_color(&settings.text_color) || !is_hex_color(&settings.translated_text_color) {
        return Err("Caption colors must be six-digit hex colors.".into());
    }
    Ok(settings)
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn configure_overlay_window(
    overlay: &WebviewWindow,
    settings: &OverlaySettings,
) -> Result<(), String> {
    overlay
        .set_ignore_cursor_events(settings.click_through)
        .map_err(|error| error.to_string())?;
    overlay
        .set_focusable(!settings.click_through)
        .map_err(|error| error.to_string())?;

    let monitor = match overlay
        .current_monitor()
        .map_err(|error| error.to_string())?
    {
        Some(monitor) => monitor,
        None => overlay
            .primary_monitor()
            .map_err(|error| error.to_string())?
            .ok_or("No monitor is available for the caption overlay.")?,
    };
    let scale_factor = monitor.scale_factor();
    let work_area = *monitor.work_area();
    let maximum_logical_width = (f64::from(work_area.size.width) / scale_factor - 32.0).max(320.0);
    let maximum_logical_height = (f64::from(work_area.size.height) / scale_factor - 32.0).max(80.0);
    let logical_width = (f64::from(settings.width) + 40.0).clamp(320.0, maximum_logical_width);
    let (bilingual_height, current_wrap_allowance) = match settings.bilingual_layout {
        BilingualLayout::Stacked => (2.0, 0.0),
        // Both columns wrap in full instead of ellipsizing history. Reserve two
        // visual lines per requested row plus room for a longer live pair; the
        // frontend drops only complete oldest pairs if content still exceeds
        // the available work area.
        BilingualLayout::SideBySide => (3.0, 3.0),
    };
    let logical_height = (f64::from(settings.font_size)
        * 1.25
        * (f64::from(settings.maximum_lines) * bilingual_height + current_wrap_allowance)
        + f64::from(settings.font_size)
            * 0.18
            * f64::from(settings.maximum_lines.saturating_sub(1))
        + 48.0)
        .clamp(80.0, maximum_logical_height);
    overlay
        .set_size(LogicalSize::new(logical_width, logical_height))
        .map_err(|error| error.to_string())?;

    let physical_size = PhysicalSize::new(
        (logical_width * scale_factor).round() as u32,
        (logical_height * scale_factor).round() as u32,
    );
    let margin = (24.0 * scale_factor).round() as i32;
    overlay
        .set_position(anchored_overlay_position(
            settings.position,
            work_area,
            physical_size,
            margin,
        ))
        .map_err(|error| error.to_string())
}

fn show_live_overlay(app: &tauri::AppHandle, state: &RuntimeState) -> Result<(), String> {
    show_overlay_with_caption(app, state, String::new())
}

fn restore_live_overlay(app: &tauri::AppHandle, state: &RuntimeState) -> Result<(), String> {
    let caption = transcription::overlay_caption(state.transcript.lock().snapshot());
    show_overlay_with_caption(app, state, caption)
}

fn show_overlay_with_caption(
    app: &tauri::AppHandle,
    state: &RuntimeState,
    caption: String,
) -> Result<(), String> {
    let overlay = app
        .get_webview_window("overlay")
        .ok_or("Caption overlay is unavailable.")?;
    let settings = state.overlay_settings.lock().clone();
    configure_overlay_window(&overlay, &settings)?;
    overlay
        .emit("overlay-settings", settings)
        .map_err(|error| error.to_string())?;
    overlay
        .emit("overlay-caption", caption)
        .map_err(|error| error.to_string())?;
    overlay.show().map_err(|error| error.to_string())
}

fn anchored_overlay_position(
    anchor: OverlayPosition,
    work_area: PhysicalRect<i32, u32>,
    overlay_size: PhysicalSize<u32>,
    margin: i32,
) -> PhysicalPosition<i32> {
    let origin_x = i64::from(work_area.position.x);
    let origin_y = i64::from(work_area.position.y);
    let width = i64::from(work_area.size.width);
    let height = i64::from(work_area.size.height);
    let overlay_width = i64::from(overlay_size.width);
    let overlay_height = i64::from(overlay_size.height);
    let margin = i64::from(margin.max(0));

    let left = origin_x + margin;
    let centered = origin_x + (width - overlay_width) / 2;
    let right = origin_x + width - overlay_width - margin;
    let top = origin_y + margin;
    let bottom = origin_y + height - overlay_height - margin;

    let (x, y) = match anchor {
        OverlayPosition::TopCenter => (centered, top),
        OverlayPosition::BottomCenter => (centered, bottom),
        OverlayPosition::BottomLeft => (left, bottom),
        OverlayPosition::BottomRight => (right, bottom),
    };
    let maximum_x = origin_x + (width - overlay_width).max(0);
    let maximum_y = origin_y + (height - overlay_height).max(0);
    PhysicalPosition::new(
        x.clamp(origin_x, maximum_x) as i32,
        y.clamp(origin_y, maximum_y) as i32,
    )
}

#[tauri::command]
fn update_overlay_settings(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
    settings: OverlaySettings,
) -> Result<(), String> {
    let settings = validated_settings(settings)?;
    let overlay = app
        .get_webview_window("overlay")
        .ok_or("Caption overlay is unavailable.")?;
    configure_overlay_window(&overlay, &settings)?;
    overlay
        .emit("overlay-settings", &settings)
        .map_err(|error| error.to_string())?;
    *state.overlay_settings.lock() = settings;
    Ok(())
}

#[tauri::command]
fn report_frontend_diagnostic(scope: String, message: String) {
    let scope: String = scope.trim().chars().take(80).collect();
    let message: String = message.trim().chars().take(2_000).collect();
    tracing::warn!(frontend_scope = %scope, %message, "frontend diagnostic");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeState::default())
        .setup(|app| {
            initialize_logging(app)?;
            let runtime = app.state::<RuntimeState>();
            models::initialize(app.handle(), &runtime.model);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            source_snapshot,
            start_capture,
            stop_capture,
            capture_status,
            transcript_snapshot,
            clear_transcript,
            models::model_status,
            models::select_speech_model,
            models::install_speech_model,
            models::remove_speech_model,
            show_appearance_window,
            close_appearance_window,
            update_overlay_settings,
            report_frontend_diagnostic,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Prollyglot");
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic payload".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_settings_reject_unsafe_ranges() {
        let settings = OverlaySettings {
            background_opacity: 1.5,
            ..OverlaySettings::default()
        };
        assert!(validated_settings(settings).is_err());

        let settings = OverlaySettings {
            reading_time_seconds: 2,
            ..OverlaySettings::default()
        };
        assert!(validated_settings(settings).is_err());

        let settings = OverlaySettings {
            fade_duration_ms: 5_001,
            ..OverlaySettings::default()
        };
        assert!(validated_settings(settings).is_err());
    }

    #[test]
    fn overlay_anchor_uses_monitor_work_area() {
        let work_area = PhysicalRect {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1_920, 1_040),
        };
        let overlay = PhysicalSize::new(760, 160);

        assert_eq!(
            anchored_overlay_position(OverlayPosition::BottomCenter, work_area, overlay, 32),
            PhysicalPosition::new(580, 848)
        );
        assert_eq!(
            anchored_overlay_position(OverlayPosition::TopCenter, work_area, overlay, 32),
            PhysicalPosition::new(580, 32)
        );
    }

    #[test]
    fn overlay_anchor_supports_negative_monitor_coordinates() {
        let work_area = PhysicalRect {
            position: PhysicalPosition::new(-1_920, -120),
            size: PhysicalSize::new(1_920, 1_040),
        };
        let overlay = PhysicalSize::new(760, 160);

        assert_eq!(
            anchored_overlay_position(OverlayPosition::BottomRight, work_area, overlay, 32),
            PhysicalPosition::new(-792, 728)
        );
    }

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
}
