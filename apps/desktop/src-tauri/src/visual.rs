use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::RecvTimeoutError;
use parking_lot::Mutex;
#[cfg(test)]
use prollyglot_application_runtime::VisualPresentationRegion;
use prollyglot_application_runtime::{
    ApplicationError, ApplicationErrorCode, CancellationToken, ErrorRecoverability, PixelRect,
    RecoveryAction, RuntimeSnapshot, SessionHealthLevel, SessionId, SessionLifecycle, SessionMode,
    SessionProgress, SessionSource, SessionSourceKind, StableVisualTextRegion, StartSessionRequest,
    VisualCaptureCapabilities, VisualCaptureGeometry, VisualCaptureSelection, VisualDetectionMode,
    VisualPresentationFrame, VisualRect, VisualRegionSelected, VisualRegionSelectorRequest,
    VisualSource, VisualSourceKind, VisualSourceSnapshot, VisualState, VisualStatus,
    VisualTextClear, VisualTextUpdate, WorkerLifetime, WorkerOutcome, WorkerReporter, WorkerRole,
    ipc,
};
use prollyglot_model_manager::{
    DEFAULT_VISUAL_OCR_MODEL_ID, DownloadProgress, ModelInstallState, ModelManager, ModelManifest,
    visual_ocr_manifest, visual_ocr_manifest_by_id,
};
use prollyglot_resource_coordinator::InferenceResourceLease;
use prollyglot_visual_ocr_rapid::{RapidOcrCancellation, RapidOcrEngine, RecognitionProfile};
use prollyglot_visual_pipeline::{
    FrameGate, FrameGateConfig, OcrEngine, OcrError, OcrObservation, StabilizerUpdate,
    TextStabilizer, TextStabilizerConfig, VisualFrame, VisualPipeline, VisualPipelineStats,
};
use prollyglot_visual_windows::{
    PickedVisualSource, StartedVisualCapture, VisualCaptureEvent,
    VisualCaptureSelection as BackendVisualCaptureSelection,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};

use crate::RuntimeState;

const VISUAL_MODEL_STATUS_EVENT: &str = "visual-model-status";
const VISUAL_STATUS_INTERVAL: Duration = Duration::from_millis(500);
const MAX_VISUAL_RESULT_AGE_MICROS: u64 = 3_000_000;
const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(25);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VisualModelPhase {
    #[default]
    NotInstalled,
    Checking,
    Downloading,
    Ready,
    Corrupt,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualModelStatus {
    pub phase: VisualModelPhase,
    pub model_id: String,
    pub display_name: String,
    pub profile: String,
    pub description: String,
    pub languages: Vec<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualModelCatalogStatus {
    pub models: Vec<VisualModelStatus>,
}

impl Default for VisualModelCatalogStatus {
    fn default() -> Self {
        match visual_ocr_manifest() {
            Ok(manifest) => Self {
                models: vec![status_for_manifest(
                    &manifest,
                    VisualModelPhase::NotInstalled,
                    0,
                    None,
                )],
            },
            Err(error) => Self {
                models: vec![VisualModelStatus {
                    phase: VisualModelPhase::Failed,
                    model_id: DEFAULT_VISUAL_OCR_MODEL_ID.into(),
                    display_name: "Multilingual visual text recognition".into(),
                    profile: "Visual OCR".into(),
                    description: "Recognizes visible text locally for screen translation.".into(),
                    languages: Vec::new(),
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    message: Some(error.to_string()),
                }],
            },
        }
    }
}

struct ActiveVisualResources {
    session_id: SessionId,
    capture: StartedVisualCapture,
    capture_events: Option<JoinHandle<()>>,
    processor: Option<JoinHandle<()>>,
    ocr_cancellation: RapidOcrCancellation,
}

impl ActiveVisualResources {
    fn cancel(&self) {
        self.ocr_cancellation.cancel();
    }

    fn stop(mut self) -> Result<(), ApplicationError> {
        self.ocr_cancellation.cancel();
        let capture_error = self.capture.stop().err().map(|error| error.to_string());
        let event_error = self
            .capture_events
            .take()
            .and_then(|worker| worker.join().err())
            .map(|panic| {
                format!(
                    "visual capture worker panicked: {}",
                    crate::panic_message(panic)
                )
            });
        let processor_error = self
            .processor
            .take()
            .and_then(|worker| worker.join().err())
            .map(|panic| {
                format!(
                    "visual OCR worker panicked: {}",
                    crate::panic_message(panic)
                )
            });
        capture_error
            .or(event_error)
            .or(processor_error)
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

struct EchoFilteringEngine {
    inner: RapidOcrEngine,
    overlay_echoes: Arc<Mutex<Vec<String>>>,
    _resource: InferenceResourceLease,
}

impl OcrEngine for EchoFilteringEngine {
    fn recognize(&mut self, frame: &VisualFrame) -> Result<Vec<OcrObservation>, OcrError> {
        let observations = self.inner.recognize(frame)?;
        let echoes = self.overlay_echoes.lock();
        Ok(observations
            .into_iter()
            .filter(|observation| !matches_overlay_echo(&observation.text, &echoes))
            .collect())
    }
}

struct VisualProcessorWorker {
    app: AppHandle,
    supervisor: Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: SharedVisualStatus,
    source: Arc<Mutex<PickedVisualSource>>,
    session_id: SessionId,
    cancellation: CancellationToken,
    frames: crossbeam_channel::Receiver<VisualFrame>,
    engine: EchoFilteringEngine,
    detection_mode: VisualDetectionMode,
}

struct VisualCaptureEventWorker {
    app: AppHandle,
    supervisor: Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: SharedVisualStatus,
    source: Arc<Mutex<PickedVisualSource>>,
    overlay_output: Arc<Mutex<VisualPresentationFrame>>,
    session_id: SessionId,
    cancellation: CancellationToken,
    events: crossbeam_channel::Receiver<VisualCaptureEvent>,
}

#[derive(Default)]
struct PublishedVisualStatus {
    runtime_revision: u32,
    session_id: Option<SessionId>,
    visual: VisualStatus,
}

impl PublishedVisualStatus {
    fn apply_snapshot(&mut self, snapshot: &RuntimeSnapshot) -> Option<VisualStatus> {
        if snapshot.revision <= self.runtime_revision {
            return None;
        }
        let next = projected_visual_status(self, snapshot);
        self.runtime_revision = snapshot.revision;
        self.session_id = (snapshot.mode == Some(SessionMode::VisualTranslation))
            .then_some(snapshot.session_id)
            .flatten();
        self.visual = next.clone();
        Some(next)
    }
}

type SharedVisualStatus = Arc<Mutex<PublishedVisualStatus>>;

#[derive(Default)]
pub struct VisualRuntime {
    catalog: Arc<Mutex<VisualModelCatalogStatus>>,
    installing: Arc<AtomicBool>,
    inspecting: Arc<AtomicBool>,
    status: SharedVisualStatus,
    resources: Arc<Mutex<Option<ActiveVisualResources>>>,
    overlay_echoes: Arc<Mutex<Vec<String>>>,
    overlay_output: Arc<Mutex<VisualPresentationFrame>>,
}

pub fn initialize(app: &AppHandle, runtime: &VisualRuntime) {
    let checking = visual_ocr_manifest()
        .map(|manifest| VisualModelCatalogStatus {
            models: vec![status_for_manifest(
                &manifest,
                VisualModelPhase::Checking,
                0,
                Some("Checking local visual recognition files…".into()),
            )],
        })
        .unwrap_or_else(|error| inspection_failure(error.to_string()));
    *runtime.catalog.lock() = checking;
    runtime.inspecting.store(true, Ordering::Release);

    let app_for_worker = app.clone();
    let catalog = Arc::clone(&runtime.catalog);
    let inspecting = Arc::clone(&runtime.inspecting);
    let spawn = thread::Builder::new()
        .name("visual-model-inspection".into())
        .spawn(move || {
            let started = Instant::now();
            let next = inspect(&app_for_worker).unwrap_or_else(inspection_failure);
            let installed = next
                .models
                .iter()
                .filter(|model| model.phase == VisualModelPhase::Ready)
                .count();
            *catalog.lock() = next.clone();
            inspecting.store(false, Ordering::Release);
            tracing::info!(
                elapsed_ms = started.elapsed().as_millis(),
                installed,
                "visual OCR model inspection completed"
            );
            publish_model(&app_for_worker, next);
        });
    if let Err(error) = spawn {
        runtime.inspecting.store(false, Ordering::Release);
        let next = inspection_failure(format!(
            "Could not start the visual model inspection worker: {error}"
        ));
        *runtime.catalog.lock() = next.clone();
        publish_model(app, next);
    }
}

pub fn is_active(state: &RuntimeState) -> bool {
    let supervisor = state.supervisor.lock();
    supervisor.has_active_session()
        && supervisor.snapshot().mode == Some(SessionMode::VisualTranslation)
}

#[tauri::command]
pub fn visual_capabilities() -> VisualCaptureCapabilities {
    let capabilities = prollyglot_visual_windows::capabilities();
    VisualCaptureCapabilities {
        windows_graphics_capture: capabilities.windows_graphics_capture,
        system_picker: capabilities.system_picker,
        desktop_duplication_experiment: capabilities.desktop_duplication_experiment,
        message: capabilities.message,
    }
}

#[tauri::command]
pub fn visual_source_snapshot() -> Result<VisualSourceSnapshot, ApplicationError> {
    let snapshot = prollyglot_visual_windows::source_snapshot().map_err(|error| {
        tracing::error!(%error, "could not enumerate visual capture sources");
        application_error(
            ApplicationErrorCode::CaptureUnavailable,
            error.to_string(),
            ErrorRecoverability::Retryable,
            RecoveryAction::Retry,
            None,
        )
    })?;
    Ok(VisualSourceSnapshot {
        windows: snapshot
            .windows
            .into_iter()
            .map(visual_source_contract)
            .collect(),
        displays: snapshot
            .displays
            .into_iter()
            .map(visual_source_contract)
            .collect(),
    })
}

#[tauri::command]
pub fn visual_status(state: State<'_, RuntimeState>) -> VisualStatus {
    state.visual.status.lock().visual.clone()
}

#[tauri::command]
pub fn visual_model_status(state: State<'_, RuntimeState>) -> VisualModelCatalogStatus {
    state.visual.catalog.lock().clone()
}

#[tauri::command]
pub fn update_visual_presentation(
    app: AppHandle,
    caller: WebviewWindow,
    state: State<'_, RuntimeState>,
    frame: VisualPresentationFrame,
) -> Result<bool, ApplicationError> {
    if caller.label() != "main" {
        return Err(application_error(
            ApplicationErrorCode::Internal,
            "Only the main Prollyglot interface may publish visual presentation frames.",
            ErrorRecoverability::NotRecoverable,
            RecoveryAction::ReportIssue,
            Some(frame.session_id),
        ));
    }
    validate_visual_presentation(&frame).map_err(|message| {
        application_error(
            ApplicationErrorCode::ConfigurationInvalid,
            message,
            ErrorRecoverability::UserActionRequired,
            RecoveryAction::OpenSettings,
            Some(frame.session_id),
        )
    })?;

    let runtime_snapshot = state.supervisor.lock().snapshot();
    if runtime_snapshot.mode != Some(SessionMode::VisualTranslation)
        || runtime_snapshot.session_id != Some(frame.session_id)
        || !matches!(
            runtime_snapshot.lifecycle,
            SessionLifecycle::Starting | SessionLifecycle::Running | SessionLifecycle::Waiting
        )
        || frame.runtime_revision > runtime_snapshot.revision
    {
        return Ok(false);
    }
    let overlay = app.get_webview_window("visual-overlay").ok_or_else(|| {
        window_operation_error(
            "Visual translation overlay is unavailable.",
            Some(frame.session_id),
        )
    })?;

    let region_count = frame.regions.len() as u64;
    let echoes = frame
        .regions
        .iter()
        .filter_map(|region| region.translation.as_deref())
        .take(48)
        .filter_map(|text| {
            let normalized = normalize_overlay_text(text);
            (normalized.chars().count() >= 4).then_some(normalized)
        })
        .collect();
    let (previous, changed) = {
        let mut current = state.visual.overlay_output.lock();
        if current.session_id != frame.session_id
            || frame.runtime_revision < current.runtime_revision
            || frame.presentation_revision <= current.presentation_revision
        {
            return Ok(false);
        }
        let changed =
            current.regions.len() != frame.regions.len() || current.scanning != frame.scanning;
        let previous = std::mem::replace(&mut *current, frame.clone());
        (previous, changed)
    };

    if let Err(error) = overlay.emit(ipc::VISUAL_PRESENTATION_EVENT, &frame) {
        *state.visual.overlay_output.lock() = previous;
        return Err(window_operation_error(
            error.to_string(),
            Some(frame.session_id),
        ));
    }
    *state.visual.overlay_echoes.lock() = echoes;

    if runtime_snapshot.lifecycle == SessionLifecycle::Running {
        overlay
            .set_always_on_top(true)
            .map_err(|error| window_operation_error(error.to_string(), Some(frame.session_id)))?;
        overlay
            .show()
            .map_err(|error| window_operation_error(error.to_string(), Some(frame.session_id)))?;
        publish_overlay_region_count(&app, &state.visual.status, frame.session_id, region_count);
    }
    if changed {
        tracing::info!(
            overlay_regions = region_count,
            scanning = frame.scanning,
            session_id = %frame.session_id,
            presentation_revision = frame.presentation_revision,
            "visual presentation delivered"
        );
    }
    Ok(true)
}

#[tauri::command]
pub fn show_visual_region_selector(
    app: AppHandle,
    display_id: String,
) -> Result<VisualRegionSelectorRequest, ApplicationError> {
    let display = prollyglot_visual_windows::source_snapshot()
        .map_err(|error| {
            application_error(
                ApplicationErrorCode::CaptureUnavailable,
                error.to_string(),
                ErrorRecoverability::Retryable,
                RecoveryAction::Retry,
                None,
            )
        })?
        .displays
        .into_iter()
        .find(|display| display.id == display_id)
        .ok_or_else(|| {
            application_error(
                ApplicationErrorCode::CaptureUnavailable,
                "The selected display is no longer available.",
                ErrorRecoverability::UserActionRequired,
                RecoveryAction::ChooseAnotherSource,
                None,
            )
        })?;
    let selector = app.get_webview_window("region-selector").ok_or_else(|| {
        window_operation_error("The visual region selector is unavailable.", None)
    })?;
    selector
        .set_ignore_cursor_events(false)
        .map_err(|error| window_operation_error(error.to_string(), None))?;
    selector
        .set_focusable(true)
        .map_err(|error| window_operation_error(error.to_string(), None))?;
    selector
        .set_position(PhysicalPosition::new(display.x, display.y))
        .map_err(|error| window_operation_error(error.to_string(), None))?;
    selector
        .set_size(PhysicalSize::new(display.width, display.height))
        .map_err(|error| window_operation_error(error.to_string(), None))?;
    selector
        .show()
        .map_err(|error| window_operation_error(error.to_string(), None))?;
    selector
        .set_focus()
        .map_err(|error| window_operation_error(error.to_string(), None))?;
    Ok(VisualRegionSelectorRequest {
        display_id: display.id,
        width: display.width,
        height: display.height,
    })
}

#[tauri::command]
pub fn complete_visual_region_selection(
    app: AppHandle,
    display_id: String,
    region: PixelRect,
) -> Result<(), ApplicationError> {
    let display = prollyglot_visual_windows::source_snapshot()
        .map_err(|error| {
            application_error(
                ApplicationErrorCode::CaptureUnavailable,
                error.to_string(),
                ErrorRecoverability::Retryable,
                RecoveryAction::Retry,
                None,
            )
        })?
        .displays
        .into_iter()
        .find(|display| display.id == display_id)
        .ok_or_else(|| {
            application_error(
                ApplicationErrorCode::CaptureUnavailable,
                "The selected display is no longer available.",
                ErrorRecoverability::UserActionRequired,
                RecoveryAction::ChooseAnotherSource,
                None,
            )
        })?;
    if !region.fits_within(display.width, display.height) {
        return Err(application_error(
            ApplicationErrorCode::ConfigurationInvalid,
            "The selected region is outside the display.",
            ErrorRecoverability::UserActionRequired,
            RecoveryAction::ChooseAnotherSource,
            None,
        ));
    }
    if let Some(selector) = app.get_webview_window("region-selector") {
        selector
            .hide()
            .map_err(|error| window_operation_error(error.to_string(), None))?;
    }
    app.emit(
        prollyglot_application_runtime::ipc::VISUAL_REGION_SELECTED_EVENT,
        VisualRegionSelected { display_id, region },
    )
    .map_err(|error| window_operation_error(error.to_string(), None))
}

#[tauri::command]
pub fn cancel_visual_region_selection(app: AppHandle) -> Result<(), ApplicationError> {
    if let Some(selector) = app.get_webview_window("region-selector") {
        selector
            .hide()
            .map_err(|error| window_operation_error(error.to_string(), None))?;
    }
    app.emit(
        prollyglot_application_runtime::ipc::VISUAL_REGION_SELECTION_CANCELLED_EVENT,
        (),
    )
    .map_err(|error| window_operation_error(error.to_string(), None))
}

#[tauri::command]
pub fn install_visual_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    model_id: String,
) -> Result<(), String> {
    let manifest = visual_ocr_manifest_by_id(&model_id).map_err(|error| error.to_string())?;
    let _control = state.control.lock();
    require_model_changes_allowed(&state)?;
    state
        .visual
        .installing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "A visual recognition model is already downloading.".to_owned())?;
    update_model(
        &app,
        &state.visual.catalog,
        &manifest.id,
        VisualModelPhase::Downloading,
        0,
        Some(format!(
            "Downloading and verifying {}…",
            manifest.display_name
        )),
    );

    let catalog = Arc::clone(&state.visual.catalog);
    let installing = Arc::clone(&state.visual.installing);
    let app_for_worker = app.clone();
    let root = match visual_models_root(&app) {
        Ok(root) => root,
        Err(message) => {
            state.visual.installing.store(false, Ordering::Release);
            update_model(
                &app,
                &state.visual.catalog,
                &model_id,
                VisualModelPhase::Failed,
                0,
                Some(message.clone()),
            );
            return Err(message);
        }
    };
    let spawn = thread::Builder::new()
        .name("visual-model-download".into())
        .spawn(move || {
            let manager = ModelManager::new(root);
            let mut last_publish = Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now);
            let result = manager.install(&manifest, |progress| {
                if last_publish.elapsed() >= Duration::from_millis(100)
                    || progress.completed_bytes == progress.model_bytes
                {
                    publish_download_progress(&app_for_worker, &catalog, progress);
                    last_publish = Instant::now();
                }
            });
            match result {
                Ok(_) => update_model(
                    &app_for_worker,
                    &catalog,
                    &manifest.id,
                    VisualModelPhase::Ready,
                    manifest.download_size_bytes(),
                    None,
                ),
                Err(error) => {
                    tracing::error!(model_id = %manifest.id, %error, "visual OCR model installation failed");
                    update_model(
                        &app_for_worker,
                        &catalog,
                        &manifest.id,
                        VisualModelPhase::Failed,
                        0,
                        Some(error.to_string()),
                    );
                }
            }
            installing.store(false, Ordering::Release);
        });
    if let Err(error) = spawn {
        state.visual.installing.store(false, Ordering::Release);
        let message = format!("Could not start the visual model download worker: {error}");
        update_model(
            &app,
            &state.visual.catalog,
            &model_id,
            VisualModelPhase::Failed,
            0,
            Some(message.clone()),
        );
        return Err(message);
    }
    Ok(())
}

#[tauri::command]
pub fn remove_visual_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    model_id: String,
) -> Result<(), String> {
    let manifest = visual_ocr_manifest_by_id(&model_id).map_err(|error| error.to_string())?;
    let _control = state.control.lock();
    require_model_changes_allowed(&state)?;
    ModelManager::new(visual_models_root(&app)?)
        .remove(&manifest)
        .map_err(|error| error.to_string())?;
    update_model(
        &app,
        &state.visual.catalog,
        &manifest.id,
        VisualModelPhase::NotInstalled,
        0,
        None,
    );
    Ok(())
}

#[tauri::command]
pub async fn start_visual_translation(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    selection: VisualCaptureSelection,
    source_language: String,
    target_language: String,
    detection_mode: Option<VisualDetectionMode>,
) -> Result<(), ApplicationError> {
    validate_language_pair(&source_language, &target_language).map_err(|message| {
        application_error(
            ApplicationErrorCode::ConfigurationInvalid,
            message,
            ErrorRecoverability::UserActionRequired,
            RecoveryAction::OpenSettings,
            None,
        )
    })?;
    let detection_mode = detection_mode.unwrap_or_default();
    let unresolved_source = visual_session_source(&selection);
    let started = {
        let _control = state.control.lock();
        state.supervisor.lock().start(StartSessionRequest {
            mode: SessionMode::VisualTranslation,
            source: unresolved_source.clone(),
        })?
    };
    {
        state.visual.overlay_echoes.lock().clear();
        *state.visual.overlay_output.lock() = VisualPresentationFrame {
            session_id: started.session_id,
            runtime_revision: started.snapshot.revision,
            presentation_revision: 0,
            source_language: source_language.clone(),
            target_language: target_language.clone(),
            scanning: true,
            ..VisualPresentationFrame::default()
        };
    }
    publish_visual_runtime(&app, &state.visual.status, started.snapshot.clone());
    spawn_session_monitor(
        VisualSessionMonitorContext {
            app: app.clone(),
            supervisor: Arc::clone(&state.supervisor),
            resources: Arc::clone(&state.visual.resources),
            status: Arc::clone(&state.visual.status),
            overlay_output: Arc::clone(&state.visual.overlay_output),
            overlay_echoes: Arc::clone(&state.visual.overlay_echoes),
            inference_resources: state.resources.clone(),
        },
        started.session_id,
    )
    .inspect_err(|error| {
        fail_and_publish(
            &app,
            &state.supervisor,
            &state.visual.status,
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
    tracing::info!(
        ?selection,
        %source_language,
        %target_language,
        ?detection_mode,
        session_id = %started.session_id,
        "starting visual translation session"
    );
    if let Ok(snapshot) = state.supervisor.lock().update_start_progress(
        started.session_id,
        SessionProgress::PreparingModel,
        Some("Loading local visual text recognition…".into()),
    ) {
        publish_visual_runtime(&app, &state.visual.status, snapshot);
    }

    let model_directory = match installed_model_directory(&app, &state.visual) {
        Ok(directory) => directory,
        Err(message) => {
            let error = application_error(
                ApplicationErrorCode::ModelUnavailable,
                message,
                ErrorRecoverability::UserActionRequired,
                RecoveryAction::InstallModel,
                Some(started.session_id),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };
    let source_language_for_worker = source_language.clone();
    let recognition_profile = match detection_mode {
        VisualDetectionMode::Focused => RecognitionProfile::Focused,
        VisualDetectionMode::AllText => RecognitionProfile::AllText,
    };
    let load_started = Instant::now();
    let engine = match tauri::async_runtime::spawn_blocking(move || {
        RapidOcrEngine::load_with_profile(
            model_directory,
            source_language_for_worker,
            recognition_profile,
        )
    })
    .await
    {
        Ok(Ok(engine)) => engine,
        Ok(Err(error)) => {
            let error = application_error(
                ApplicationErrorCode::ModelFailed,
                error.to_string(),
                ErrorRecoverability::Retryable,
                RecoveryAction::InstallModel,
                Some(started.session_id),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
        Err(join_error) => {
            let error = application_error(
                ApplicationErrorCode::WorkerPanicked,
                format!("Could not join the visual model-loading worker: {join_error}"),
                ErrorRecoverability::Retryable,
                RecoveryAction::StopAndRetry,
                Some(started.session_id),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Panicked);
            return Err(error);
        }
    };
    tracing::info!(
        elapsed_ms = load_started.elapsed().as_millis(),
        %source_language,
        %target_language,
        ?detection_mode,
        "visual OCR model ready"
    );
    let ocr_cancellation = engine.cancellation();

    if started.cancellation.is_cancelled() {
        ocr_cancellation.cancel();
        finish_reporter(&mut startup_reporter, WorkerOutcome::Cancelled);
        return Err(startup_cancelled(started.session_id));
    }
    let ocr_resource = match state.resources.acquire(
        started.session_id,
        SessionMode::VisualTranslation,
        prollyglot_application_runtime::InferenceResourceKind::VisualOcr,
        DEFAULT_VISUAL_OCR_MODEL_ID,
        load_started.elapsed().as_millis() as u64,
    ) {
        Ok(resource) => resource,
        Err(error) => {
            ocr_cancellation.cancel();
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };

    if let Ok(snapshot) = state.supervisor.lock().update_start_progress(
        started.session_id,
        SessionProgress::StartingCapture,
        None,
    ) {
        publish_visual_runtime(&app, &state.visual.status, snapshot);
    }

    let capture =
        match prollyglot_visual_windows::start_capture(backend_visual_selection(&selection)) {
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
    if started.cancellation.is_cancelled() {
        schedule_cleanup(
            &app,
            &state.supervisor,
            &state.visual.status,
            &state.visual.overlay_output,
            &state.visual.overlay_echoes,
            started.session_id,
            Some(ActiveVisualResources {
                session_id: started.session_id,
                capture,
                capture_events: None,
                processor: None,
                ocr_cancellation,
            }),
        );
        finish_reporter(&mut startup_reporter, WorkerOutcome::Cancelled);
        return Err(startup_cancelled(started.session_id));
    }
    let source = Arc::new(Mutex::new(capture.source.clone()));
    let resolved_source = SessionSource {
        label: capture.source.label.clone(),
        ..unresolved_source
    };
    if let Ok(snapshot) = state
        .supervisor
        .lock()
        .update_source(started.session_id, resolved_source)
    {
        publish_visual_runtime(&app, &state.visual.status, snapshot);
    }
    if let Err(message) =
        configure_visual_overlay(&app, &capture.source, &state.visual.overlay_output, false)
    {
        let error = application_error(
            ApplicationErrorCode::WindowOperationFailed,
            message,
            ErrorRecoverability::Retryable,
            RecoveryAction::StopAndRetry,
            Some(started.session_id),
        );
        schedule_cleanup(
            &app,
            &state.supervisor,
            &state.visual.status,
            &state.visual.overlay_output,
            &state.visual.overlay_echoes,
            started.session_id,
            Some(ActiveVisualResources {
                session_id: started.session_id,
                capture,
                capture_events: None,
                processor: None,
                ocr_cancellation,
            }),
        );
        finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
        return Err(error);
    }

    let processor_reporter = match state.supervisor.lock().register_worker(
        started.session_id,
        WorkerRole::VisualRecognition,
        WorkerLifetime::Session,
    ) {
        Ok(reporter) => reporter,
        Err(error) => {
            schedule_cleanup(
                &app,
                &state.supervisor,
                &state.visual.status,
                &state.visual.overlay_output,
                &state.visual.overlay_echoes,
                started.session_id,
                Some(ActiveVisualResources {
                    session_id: started.session_id,
                    capture,
                    capture_events: None,
                    processor: None,
                    ocr_cancellation,
                }),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };
    let processor = match spawn_processor(
        VisualProcessorWorker {
            app: app.clone(),
            supervisor: Arc::clone(&state.supervisor),
            status: Arc::clone(&state.visual.status),
            source: Arc::clone(&source),
            session_id: started.session_id,
            cancellation: started.cancellation.clone(),
            frames: capture.frames.clone(),
            engine: EchoFilteringEngine {
                inner: engine,
                overlay_echoes: Arc::clone(&state.visual.overlay_echoes),
                _resource: ocr_resource,
            },
            detection_mode,
        },
        processor_reporter,
    ) {
        Ok(processor) => processor,
        Err(error) => {
            schedule_cleanup(
                &app,
                &state.supervisor,
                &state.visual.status,
                &state.visual.overlay_output,
                &state.visual.overlay_echoes,
                started.session_id,
                Some(ActiveVisualResources {
                    session_id: started.session_id,
                    capture,
                    capture_events: None,
                    processor: None,
                    ocr_cancellation,
                }),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };
    let event_reporter = match state.supervisor.lock().register_worker(
        started.session_id,
        WorkerRole::VisualEvents,
        WorkerLifetime::Session,
    ) {
        Ok(reporter) => reporter,
        Err(error) => {
            schedule_cleanup(
                &app,
                &state.supervisor,
                &state.visual.status,
                &state.visual.overlay_output,
                &state.visual.overlay_echoes,
                started.session_id,
                Some(ActiveVisualResources {
                    session_id: started.session_id,
                    capture,
                    capture_events: None,
                    processor: Some(processor),
                    ocr_cancellation,
                }),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };
    let capture_events = match spawn_capture_events(
        VisualCaptureEventWorker {
            app: app.clone(),
            supervisor: Arc::clone(&state.supervisor),
            status: Arc::clone(&state.visual.status),
            source: Arc::clone(&source),
            overlay_output: Arc::clone(&state.visual.overlay_output),
            session_id: started.session_id,
            cancellation: started.cancellation.clone(),
            events: capture.events.clone(),
        },
        event_reporter,
    ) {
        Ok(worker) => worker,
        Err(error) => {
            schedule_cleanup(
                &app,
                &state.supervisor,
                &state.visual.status,
                &state.visual.overlay_output,
                &state.visual.overlay_echoes,
                started.session_id,
                Some(ActiveVisualResources {
                    session_id: started.session_id,
                    capture,
                    capture_events: None,
                    processor: Some(processor),
                    ocr_cancellation,
                }),
            );
            finish_reporter(&mut startup_reporter, WorkerOutcome::Failed(error.clone()));
            return Err(error);
        }
    };

    let resources = ActiveVisualResources {
        session_id: started.session_id,
        capture,
        capture_events: Some(capture_events),
        processor: Some(processor),
        ocr_cancellation,
    };
    let resources = {
        let _control = state.control.lock();
        if started.cancellation.is_cancelled() {
            Some(resources)
        } else {
            let mut slot = state.visual.resources.lock();
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
            &state.visual.status,
            &state.visual.overlay_output,
            &state.visual.overlay_echoes,
            started.session_id,
            Some(resources),
        );
        finish_reporter(&mut startup_reporter, WorkerOutcome::Cancelled);
        return Err(startup_cancelled(started.session_id));
    }

    finish_reporter(&mut startup_reporter, WorkerOutcome::Completed);
    let lifecycle = state.supervisor.lock().snapshot().lifecycle;
    if lifecycle == SessionLifecycle::Starting {
        let running = state.supervisor.lock().mark_running(started.session_id)?;
        publish_visual_runtime(&app, &state.visual.status, running);
    } else if lifecycle != SessionLifecycle::Waiting {
        return Err(startup_cancelled(started.session_id));
    }
    let overlay_result = {
        let _control = state.control.lock();
        if started.cancellation.is_cancelled() {
            return Err(startup_cancelled(started.session_id));
        }
        if state.supervisor.lock().snapshot().lifecycle == SessionLifecycle::Running {
            configure_visual_overlay(&app, &source.lock(), &state.visual.overlay_output, true)
        } else {
            Ok(())
        }
    };
    let message = format!(
        "Watching the live source, recognizing {source_language} text, and translating to {target_language}."
    );
    let session_is_running =
        state.supervisor.lock().snapshot().lifecycle == SessionLifecycle::Running;
    if session_is_running
        && let Ok(snapshot) = state.supervisor.lock().update_health(
            started.session_id,
            SessionHealthLevel::Healthy,
            Some(message),
        )
    {
        publish_visual_runtime(&app, &state.visual.status, snapshot);
    }
    if let Err(message) = overlay_result {
        tracing::warn!(%message, "could not show visual translation overlay");
        if let Ok(snapshot) = state.supervisor.lock().update_health(
            started.session_id,
            SessionHealthLevel::Degraded,
            Some(message),
        ) {
            publish_visual_runtime(&app, &state.visual.status, snapshot);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn stop_visual_translation(
    app: AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<(), ApplicationError> {
    let (permit, resources) = {
        let _control = state.control.lock();
        let permit = state
            .supervisor
            .lock()
            .request_stop_for_mode(SessionMode::VisualTranslation)?;
        let resources = take_resources(&state.visual.resources, permit.session_id);
        if let Some(resources) = resources.as_ref() {
            resources.cancel();
        }
        (permit, resources)
    };
    let revision = permit.snapshot.revision;
    publish_visual_runtime(&app, &state.visual.status, permit.snapshot);
    clear_visual_output(
        &app,
        &state.visual.overlay_output,
        &state.visual.overlay_echoes,
        permit.session_id,
        revision,
    );
    if !permit.already_stopping || resources.is_some() {
        schedule_cleanup(
            &app,
            &state.supervisor,
            &state.visual.status,
            &state.visual.overlay_output,
            &state.visual.overlay_echoes,
            permit.session_id,
            resources,
        );
    }
    Ok(())
}

fn spawn_processor(
    worker: VisualProcessorWorker,
    reporter: WorkerReporter,
) -> Result<JoinHandle<()>, ApplicationError> {
    let session_id = worker.session_id;
    let cancellation = worker.cancellation.clone();
    thread::Builder::new()
        .name("visual-ocr".into())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_visual_processor(worker)
            }));
            let outcome = match outcome {
                Ok(Ok(())) if cancellation.is_cancelled() => WorkerOutcome::Cancelled,
                Ok(Ok(())) => WorkerOutcome::Completed,
                Ok(Err(error)) => WorkerOutcome::Failed(error),
                Err(_) => WorkerOutcome::Panicked,
            };
            reporter.finish(outcome);
        })
        .map_err(|error| {
            application_error(
                ApplicationErrorCode::WorkerExited,
                format!("Could not start the visual OCR worker: {error}"),
                ErrorRecoverability::Retryable,
                RecoveryAction::StopAndRetry,
                Some(session_id),
            )
        })
}

fn run_visual_processor(worker: VisualProcessorWorker) -> Result<(), ApplicationError> {
    let VisualProcessorWorker {
        app,
        supervisor,
        status,
        source,
        session_id,
        cancellation,
        frames,
        engine,
        detection_mode,
    } = worker;
    let app = &app;
    let supervisor = &supervisor;
    let status = &status;
    let source = &source;
    let cancellation = &cancellation;
    let frames = &frames;
    let stabilizer_config = match detection_mode {
        VisualDetectionMode::Focused => TextStabilizerConfig {
            required_consecutive_frames: 1,
            ..TextStabilizerConfig::default()
        },
        VisualDetectionMode::AllText => TextStabilizerConfig::default(),
    };
    let mut pipeline = VisualPipeline::new(
        FrameGate::new(FrameGateConfig::default()),
        engine,
        TextStabilizer::new(stabilizer_config),
    );
    let mut last_status_publish = Instant::now()
        .checked_sub(VISUAL_STATUS_INTERVAL)
        .unwrap_or_else(Instant::now);
    let mut last_slow_pass_log = Instant::now()
        .checked_sub(Duration::from_secs(5))
        .unwrap_or_else(Instant::now);
    let mut pending_frame = None;
    loop {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        let mut frame = match pending_frame.take() {
            Some(frame) => frame,
            None => match frames.recv_timeout(Duration::from_millis(50)) {
                Ok(frame) => frame,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    if !session_is_waiting(supervisor, session_id) {
                        let message = "The selected visual source is no longer providing frames. Stop visual translation or choose another source.";
                        if let Some(revision) =
                            mark_visual_waiting(app, supervisor, status, session_id, message)
                        {
                            emit_visual_clear(app, session_id, revision);
                            hide_visual_overlay(app);
                        }
                    }
                    thread::sleep(Duration::from_millis(50));
                    continue;
                }
            },
        };
        while let Ok(newer) = frames.try_recv() {
            frame = newer;
        }
        let pass_started = Instant::now();
        let mut outcome = match pipeline.process(&frame) {
            Ok(outcome) => outcome,
            Err(OcrError::Cancelled) if cancellation.is_cancelled() => return Ok(()),
            Err(error) => {
                tracing::error!(%error, session_id = %session_id, "visual OCR inference failed");
                return Err(application_error(
                    ApplicationErrorCode::ModelFailed,
                    error.to_string(),
                    ErrorRecoverability::Retryable,
                    RecoveryAction::StopAndRetry,
                    Some(session_id),
                ));
            }
        };
        let pass_elapsed = pass_started.elapsed();
        let newest_frame = frames.try_recv().ok();
        let result_age_micros = newest_frame.as_ref().map_or(0, |newest| {
            newest
                .captured_at_micros
                .saturating_sub(frame.captured_at_micros)
        });
        let stale_for_changed_source = outcome.update.is_some()
            && result_age_micros > MAX_VISUAL_RESULT_AGE_MICROS
            && newest_frame.as_ref().is_some_and(|newest| {
                pipeline.source_substantially_changed_since_last_analysis(newest)
            });
        if outcome.update.is_some()
            && pass_elapsed >= Duration::from_millis(750)
            && last_slow_pass_log.elapsed() >= Duration::from_secs(5)
        {
            tracing::warn!(
                elapsed_ms = pass_elapsed.as_millis(),
                frames_received = outcome.stats.frames_received,
                frames_analyzed = outcome.stats.frames_analyzed,
                stable_regions = outcome.stats.stable_regions,
                session_id = %session_id,
                "visual OCR pass is slower than the live-media target"
            );
            last_slow_pass_log = Instant::now();
        }
        let snapshot = supervisor.lock().snapshot();
        if snapshot.session_id != Some(session_id)
            || !matches!(
                snapshot.lifecycle,
                SessionLifecycle::Starting | SessionLifecycle::Running | SessionLifecycle::Waiting
            )
        {
            return Ok(());
        }
        if (stale_for_changed_source || outcome.update.is_some())
            && snapshot.lifecycle != SessionLifecycle::Waiting
        {
            if stale_for_changed_source {
                pipeline.reset_text_tracks();
                outcome.stats.stable_regions = 0;
                emit_visual_clear(app, session_id, snapshot.revision);
                tracing::warn!(
                    result_age_ms = result_age_micros / 1_000,
                    session_id = %session_id,
                    "discarded stale visual OCR output after the source changed"
                );
            } else if let Some(update) = outcome.update {
                let payload =
                    visual_text_contract(session_id, snapshot.revision, &source.lock(), update);
                if let Err(error) = app.emit_to(
                    "main",
                    prollyglot_application_runtime::ipc::VISUAL_TEXT_EVENT,
                    payload,
                ) {
                    tracing::warn!(%error, "could not emit visual text update");
                }
            }
        }
        if last_status_publish.elapsed() >= VISUAL_STATUS_INTERVAL {
            publish_pipeline_stats(app, status, session_id, outcome.stats);
            last_status_publish = Instant::now();
        }
        pending_frame = newest_frame;
    }
}

fn spawn_capture_events(
    worker: VisualCaptureEventWorker,
    reporter: WorkerReporter,
) -> Result<JoinHandle<()>, ApplicationError> {
    let session_id = worker.session_id;
    thread::Builder::new()
        .name("visual-capture-events".into())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_visual_capture_events(&worker)
            }));
            let outcome = match outcome {
                Ok(Ok(())) if worker.cancellation.is_cancelled() => WorkerOutcome::Cancelled,
                Ok(Ok(())) => WorkerOutcome::Completed,
                Ok(Err(error)) => WorkerOutcome::Failed(error),
                Err(_) => WorkerOutcome::Panicked,
            };
            reporter.finish(outcome);
        })
        .map_err(|error| {
            application_error(
                ApplicationErrorCode::WorkerExited,
                format!("Could not start the visual capture event worker: {error}"),
                ErrorRecoverability::Retryable,
                RecoveryAction::StopAndRetry,
                Some(session_id),
            )
        })
}

fn run_visual_capture_events(worker: &VisualCaptureEventWorker) -> Result<(), ApplicationError> {
    let VisualCaptureEventWorker {
        app,
        supervisor,
        status,
        source,
        overlay_output,
        session_id,
        cancellation,
        events,
    } = worker;
    let session_id = *session_id;
    let mut last_status_publish = Instant::now()
        .checked_sub(VISUAL_STATUS_INTERVAL)
        .unwrap_or_else(Instant::now);
    let mut source_closed = false;
    loop {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        let event = match events.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected)
                if source_closed || session_is_waiting(supervisor, session_id) =>
            {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                source_closed = true;
                let message = "The selected visual source is no longer available. Stop visual translation or choose another source.";
                if let Some(revision) =
                    mark_visual_waiting(app, supervisor, status, session_id, message)
                {
                    emit_visual_clear(app, session_id, revision);
                    hide_visual_overlay(app);
                }
                thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        let snapshot = supervisor.lock().snapshot();
        if snapshot.session_id != Some(session_id)
            || matches!(
                snapshot.lifecycle,
                SessionLifecycle::Stopping | SessionLifecycle::Failed | SessionLifecycle::Stopped
            )
        {
            return Ok(());
        }
        match event {
            VisualCaptureEvent::Started(next_source) => {
                source_closed = false;
                *source.lock() = next_source.clone();
                resume_visual_if_needed(app, supervisor, status, session_id);
                let show = session_is_running(supervisor, session_id);
                if let Err(error) =
                    configure_visual_overlay(app, &next_source, overlay_output, show)
                {
                    tracing::warn!(%error, "could not configure visual overlay");
                }
            }
            VisualCaptureEvent::Frame {
                x,
                y,
                width,
                height,
                replaced_frames,
                ..
            } => {
                source_closed = false;
                resume_visual_if_needed(app, supervisor, status, session_id);
                let changed = {
                    let mut current = source.lock();
                    let changed = current.x != x
                        || current.y != y
                        || current.width != width
                        || current.height != height;
                    current.x = x;
                    current.y = y;
                    current.width = width;
                    current.height = height;
                    changed
                };
                if changed {
                    let next_source = source.lock().clone();
                    if let Err(error) = configure_visual_overlay(
                        app,
                        &next_source,
                        overlay_output,
                        session_is_running(supervisor, session_id),
                    ) {
                        tracing::warn!(%error, "could not follow visual source geometry");
                    }
                }
                if changed || last_status_publish.elapsed() >= VISUAL_STATUS_INTERVAL {
                    publish_replaced_frames(app, status, session_id, replaced_frames);
                    last_status_publish = Instant::now();
                }
            }
            VisualCaptureEvent::SourceClosed => {
                source_closed = true;
                let message = "The selected visual source closed. Stop visual translation or choose another source.";
                tracing::warn!(%message, session_id = %session_id, "visual source closed");
                if let Some(revision) =
                    mark_visual_waiting(app, supervisor, status, session_id, message)
                {
                    emit_visual_clear(app, session_id, revision);
                }
                hide_visual_overlay(app);
            }
        }
    }
}

fn publish_pipeline_stats(
    app: &AppHandle,
    status: &SharedVisualStatus,
    session_id: SessionId,
    stats: VisualPipelineStats,
) {
    let next = {
        let mut published = status.lock();
        if published.session_id != Some(session_id)
            || published.visual.state != VisualState::Capturing
            || !published.visual.active
        {
            return;
        }
        published.visual.frames_received = stats.frames_received;
        published.visual.frames_analyzed = stats.frames_analyzed;
        published.visual.frames_unchanged = stats.frames_unchanged;
        published.visual.visible_regions = stats.stable_regions;
        published.visual.clone()
    };
    emit_status(app, next);
}

fn publish_overlay_region_count(
    app: &AppHandle,
    status: &SharedVisualStatus,
    session_id: SessionId,
    region_count: u64,
) {
    let next = {
        let mut published = status.lock();
        if published.session_id != Some(session_id)
            || published.visual.state != VisualState::Capturing
            || !published.visual.active
            || published.visual.overlay_regions == region_count
        {
            return;
        }
        published.visual.overlay_regions = region_count;
        published.visual.clone()
    };
    emit_status(app, next);
}

fn publish_replaced_frames(
    app: &AppHandle,
    status: &SharedVisualStatus,
    session_id: SessionId,
    replaced_frames: u64,
) {
    let next = {
        let mut published = status.lock();
        if published.session_id != Some(session_id)
            || published.visual.state != VisualState::Capturing
            || !published.visual.active
        {
            return;
        }
        published.visual.replaced_frames = replaced_frames;
        published.visual.clone()
    };
    emit_status(app, next);
}

fn configure_visual_overlay(
    app: &AppHandle,
    source: &PickedVisualSource,
    output: &Arc<Mutex<VisualPresentationFrame>>,
    show: bool,
) -> Result<(), String> {
    let overlay = app
        .get_webview_window("visual-overlay")
        .ok_or("Visual translation overlay is unavailable.")?;
    overlay
        .set_ignore_cursor_events(true)
        .map_err(|error| error.to_string())?;
    overlay
        .set_focusable(false)
        .map_err(|error| error.to_string())?;
    overlay
        .set_always_on_top(true)
        .map_err(|error| error.to_string())?;
    overlay
        .set_position(PhysicalPosition::new(source.x, source.y))
        .map_err(|error| error.to_string())?;
    overlay
        .set_size(PhysicalSize::new(source.width, source.height))
        .map_err(|error| error.to_string())?;
    overlay
        .emit(ipc::VISUAL_PRESENTATION_EVENT, output.lock().clone())
        .map_err(|error| error.to_string())?;
    if show {
        overlay.show().map_err(|error| error.to_string())
    } else {
        overlay.hide().map_err(|error| error.to_string())
    }
}

fn validate_visual_presentation(frame: &VisualPresentationFrame) -> Result<(), String> {
    const JS_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
    if frame.session_id.0 == 0 {
        return Err("Visual presentation requires an active session identifier.".into());
    }
    if frame.presentation_revision == 0 || frame.presentation_revision > JS_MAX_SAFE_INTEGER {
        return Err("The visual presentation revision must be a safe integer.".into());
    }
    if frame.source_width == 0 || frame.source_height == 0 {
        return Err("Visual presentation source dimensions must be non-zero.".into());
    }
    if frame.source_language.is_empty()
        || frame.target_language.is_empty()
        || frame.source_language.chars().count() > 16
        || frame.target_language.chars().count() > 16
    {
        return Err("Visual presentation language identifiers are invalid.".into());
    }
    if frame.regions.len() > 48 {
        return Err("Visual presentation contains too many regions.".into());
    }
    if frame.regions.iter().any(|region| {
        region.track_id == 0
            || region.track_id > JS_MAX_SAFE_INTEGER
            || region.text_revision == 0
            || region.text_revision > JS_MAX_SAFE_INTEGER
            || region.original.trim().is_empty()
            || region.original.chars().count() > 2_000
            || region.translation.as_ref().is_some_and(|translation| {
                translation.trim().is_empty() || translation.chars().count() > 2_000
            })
            || !region.bounds.is_valid()
            || (region.translation_pending && region.translation.is_some())
    }) {
        return Err("Visual presentation contains an invalid region.".into());
    }
    Ok(())
}

fn normalize_overlay_text(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .take(500)
        .collect()
}

fn matches_overlay_echo(text: &str, echoes: &[String]) -> bool {
    let candidate = normalize_overlay_text(text);
    let candidate_length = candidate.chars().count();
    if candidate_length < 4 {
        return false;
    }
    echoes.iter().any(|echo| {
        let echo_length = echo.chars().count();
        echo_length >= 4
            && (candidate == *echo
                || (candidate_length >= 6
                    && (candidate.contains(echo) || echo.contains(&candidate))))
    })
}

fn validate_language_pair(source: &str, target: &str) -> Result<(), String> {
    let manifest = visual_ocr_manifest().map_err(|error| error.to_string())?;
    if !manifest.languages.iter().any(|language| language == source) {
        return Err("The visual recognition model does not support that source language.".into());
    }
    if !manifest.languages.iter().any(|language| language == target) {
        return Err("The selected visual translation target is unsupported.".into());
    }
    if source == target {
        return Err("Choose a different target language for visual translation.".into());
    }
    Ok(())
}

fn require_model_changes_allowed(state: &RuntimeState) -> Result<(), String> {
    if state.visual.inspecting.load(Ordering::Acquire) {
        return Err("Wait for Prollyglot to finish checking the visual recognition model.".into());
    }
    if state.visual.installing.load(Ordering::Acquire) {
        return Err("Wait for the visual recognition model download to finish.".into());
    }
    if state.supervisor.lock().has_active_session() {
        return Err("Stop captions and visual translation before changing visual models.".into());
    }
    Ok(())
}

fn installed_model_directory(app: &AppHandle, runtime: &VisualRuntime) -> Result<PathBuf, String> {
    let model = runtime
        .catalog
        .lock()
        .models
        .iter()
        .find(|model| model.model_id == DEFAULT_VISUAL_OCR_MODEL_ID)
        .cloned()
        .ok_or("The visual recognition model is unavailable.")?;
    if model.phase != VisualModelPhase::Ready {
        return Err(format!(
            "Install {} in Settings before starting visual translation.",
            model.display_name
        ));
    }
    let manifest = visual_ocr_manifest().map_err(|error| error.to_string())?;
    ModelManager::new(visual_models_root(app)?)
        .location(&manifest)
        .map(|location| location.directory)
        .map_err(|error| error.to_string())
}

fn visual_models_root(app: &AppHandle) -> Result<PathBuf, String> {
    super::models::models_root(app).map(|root| root.join("visual"))
}

fn inspect(app: &AppHandle) -> Result<VisualModelCatalogStatus, String> {
    let manifest = visual_ocr_manifest().map_err(|error| error.to_string())?;
    let manager = ModelManager::new(visual_models_root(app)?);
    let status = match manager.state(&manifest) {
        Ok(ModelInstallState::NotInstalled) => {
            status_for_manifest(&manifest, VisualModelPhase::NotInstalled, 0, None)
        }
        Ok(ModelInstallState::Ready) => status_for_manifest(
            &manifest,
            VisualModelPhase::Ready,
            manifest.download_size_bytes(),
            None,
        ),
        Ok(ModelInstallState::Corrupt { issues }) => status_for_manifest(
            &manifest,
            VisualModelPhase::Corrupt,
            0,
            Some(issues.join("; ")),
        ),
        Err(error) => status_for_manifest(
            &manifest,
            VisualModelPhase::Failed,
            0,
            Some(error.to_string()),
        ),
    };
    Ok(VisualModelCatalogStatus {
        models: vec![status],
    })
}

fn inspection_failure(message: String) -> VisualModelCatalogStatus {
    let mut fallback = VisualModelCatalogStatus::default();
    if let Some(model) = fallback.models.first_mut() {
        model.phase = VisualModelPhase::Failed;
        model.message = Some(message);
    }
    fallback
}

fn status_for_manifest(
    manifest: &ModelManifest,
    phase: VisualModelPhase,
    downloaded_bytes: u64,
    message: Option<String>,
) -> VisualModelStatus {
    VisualModelStatus {
        phase,
        model_id: manifest.id.clone(),
        display_name: manifest.display_name.clone(),
        profile: "Visual OCR · Balanced".into(),
        description: "Detects and recognizes text already visible in applications, video, games, and display regions. One unified local pack covers the languages listed here.".into(),
        languages: manifest.languages.clone(),
        downloaded_bytes,
        total_bytes: manifest.download_size_bytes(),
        message,
    }
}

fn update_model(
    app: &AppHandle,
    catalog: &Arc<Mutex<VisualModelCatalogStatus>>,
    model_id: &str,
    phase: VisualModelPhase,
    downloaded_bytes: u64,
    message: Option<String>,
) {
    let next = {
        let mut current = catalog.lock();
        if let Some(model) = current
            .models
            .iter_mut()
            .find(|model| model.model_id == model_id)
        {
            model.phase = phase;
            model.downloaded_bytes = downloaded_bytes;
            model.message = message;
        }
        current.clone()
    };
    publish_model(app, next);
}

fn publish_download_progress(
    app: &AppHandle,
    catalog: &Arc<Mutex<VisualModelCatalogStatus>>,
    progress: DownloadProgress,
) {
    update_model(
        app,
        catalog,
        &progress.model_id,
        VisualModelPhase::Downloading,
        progress.completed_bytes,
        Some(format!(
            "Downloading and verifying {}…",
            progress.artifact_role
        )),
    );
}

fn publish_model(app: &AppHandle, catalog: VisualModelCatalogStatus) {
    if let Err(error) = app.emit(VISUAL_MODEL_STATUS_EVENT, catalog) {
        tracing::warn!(%error, "could not emit visual model status");
    }
}

fn emit_status(app: &AppHandle, next: VisualStatus) {
    if let Err(error) = app.emit(
        prollyglot_application_runtime::ipc::VISUAL_STATUS_EVENT,
        next,
    ) {
        tracing::warn!(%error, "could not emit visual translation status");
    }
}

fn emit_visual_clear(app: &AppHandle, session_id: SessionId, runtime_revision: u32) {
    if let Err(error) = app.emit_to(
        "main",
        prollyglot_application_runtime::ipc::VISUAL_CLEAR_EVENT,
        VisualTextClear {
            session_id,
            runtime_revision,
        },
    ) {
        tracing::warn!(%error, "could not clear visual text state");
    }
}

fn publish_visual_runtime(app: &AppHandle, status: &SharedVisualStatus, snapshot: RuntimeSnapshot) {
    if !crate::runtime::publish_snapshot(app, &snapshot) {
        return;
    }
    let mut published = status.lock();
    let published_revision = published.runtime_revision;
    let Some(next) = published.apply_snapshot(&snapshot) else {
        tracing::debug!(
            revision = snapshot.revision,
            published_revision,
            "ignored an out-of-order visual status projection"
        );
        return;
    };
    emit_status(app, next);
}

fn projected_visual_status(
    published: &PublishedVisualStatus,
    snapshot: &RuntimeSnapshot,
) -> VisualStatus {
    if snapshot.mode != Some(SessionMode::VisualTranslation) {
        return VisualStatus::default();
    }
    let mut next = if published.session_id == snapshot.session_id {
        published.visual.clone()
    } else {
        VisualStatus::default()
    };
    next.state = match snapshot.lifecycle {
        SessionLifecycle::Stopped => VisualState::Stopped,
        SessionLifecycle::Starting => VisualState::Starting,
        SessionLifecycle::Running => VisualState::Capturing,
        SessionLifecycle::Waiting => VisualState::Waiting,
        SessionLifecycle::Stopping => VisualState::Stopping,
        SessionLifecycle::Failed => VisualState::Failed,
    };
    next.active = matches!(
        snapshot.lifecycle,
        SessionLifecycle::Starting
            | SessionLifecycle::Running
            | SessionLifecycle::Waiting
            | SessionLifecycle::Stopping
    );
    next.source_label = snapshot.source.as_ref().map(|source| source.label.clone());
    next.message = snapshot
        .failure
        .as_ref()
        .map(|failure| failure.message.clone())
        .or_else(|| snapshot.health.message.clone());
    if snapshot.lifecycle == SessionLifecycle::Stopped {
        VisualStatus::default()
    } else {
        next
    }
}

fn fail_and_publish(
    app: &AppHandle,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: &SharedVisualStatus,
    session_id: SessionId,
    error: ApplicationError,
) {
    tracing::error!(%error, session_id = %session_id, "visual translation session failed");
    if let Ok(snapshot) = supervisor.lock().fail(session_id, error) {
        publish_visual_runtime(app, status, snapshot);
    }
}

struct VisualSessionMonitorContext {
    app: AppHandle,
    supervisor: Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    resources: Arc<Mutex<Option<ActiveVisualResources>>>,
    status: SharedVisualStatus,
    overlay_output: Arc<Mutex<VisualPresentationFrame>>,
    overlay_echoes: Arc<Mutex<Vec<String>>>,
    inference_resources: crate::resources::ResourceRuntime,
}

fn spawn_session_monitor(
    context: VisualSessionMonitorContext,
    session_id: SessionId,
) -> Result<(), ApplicationError> {
    let VisualSessionMonitorContext {
        app,
        supervisor,
        resources,
        status,
        overlay_output,
        overlay_echoes,
        inference_resources,
    } = context;
    thread::Builder::new()
        .name(format!("visual-session-supervisor-{session_id}"))
        .spawn(move || {
            let mut stopping_since = None::<Instant>;
            let mut failure_cleanup_started = false;
            loop {
                let updates = supervisor.lock().drain_worker_completions();
                for update in updates {
                    publish_visual_runtime(&app, &status, update);
                }
                let (snapshot, active) = {
                    let supervisor = supervisor.lock();
                    (supervisor.snapshot(), supervisor.has_active_session())
                };
                if !active || snapshot.session_id != Some(session_id) {
                    inference_resources.release_session(session_id);
                    break;
                }
                if snapshot.lifecycle == SessionLifecycle::Stopping {
                    let started = stopping_since.get_or_insert_with(Instant::now);
                    if started.elapsed() >= SHUTDOWN_TIMEOUT
                        && let Ok(snapshot) = supervisor.lock().shutdown_timed_out(session_id)
                    {
                        publish_visual_runtime(&app, &status, snapshot);
                    }
                } else {
                    stopping_since = None;
                }
                if snapshot.lifecycle == SessionLifecycle::Failed && !failure_cleanup_started {
                    failure_cleanup_started = true;
                    let resources = take_resources(&resources, session_id);
                    schedule_cleanup(
                        &app,
                        &supervisor,
                        &status,
                        &overlay_output,
                        &overlay_echoes,
                        session_id,
                        resources,
                    );
                }
                thread::sleep(SUPERVISOR_POLL_INTERVAL);
            }
        })
        .map(|_| ())
        .map_err(|error| {
            application_error(
                ApplicationErrorCode::WorkerExited,
                format!("Could not start the visual session supervisor: {error}"),
                ErrorRecoverability::RestartRequired,
                RecoveryAction::RestartApplication,
                Some(session_id),
            )
        })
}

#[allow(clippy::too_many_arguments)]
fn schedule_cleanup(
    app: &AppHandle,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: &SharedVisualStatus,
    overlay_output: &Arc<Mutex<VisualPresentationFrame>>,
    overlay_echoes: &Arc<Mutex<Vec<String>>>,
    session_id: SessionId,
    resources: Option<ActiveVisualResources>,
) {
    let revision = supervisor.lock().snapshot().revision;
    clear_visual_output(app, overlay_output, overlay_echoes, session_id, revision);
    let Some(resources) = resources else {
        if let Ok(Some(snapshot)) = supervisor.lock().finish_cleanup(session_id, Ok(())) {
            publish_visual_runtime(app, status, snapshot);
        }
        return;
    };
    resources.cancel();
    let reporter = match supervisor.lock().register_worker(
        session_id,
        WorkerRole::Shutdown,
        WorkerLifetime::Shutdown,
    ) {
        Ok(reporter) => reporter,
        Err(error) => {
            tracing::error!(%error, session_id = %session_id, "could not supervise visual cleanup");
            let _ = thread::Builder::new()
                .name("visual-cleanup-untracked".into())
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
        .name("visual-session-stop".into())
        .spawn(move || {
            let result = resources.stop();
            reporter.finish(WorkerOutcome::Completed);
            match supervisor_for_worker
                .lock()
                .finish_cleanup(session_id, result)
            {
                Ok(Some(snapshot)) => {
                    publish_visual_runtime(&app_for_worker, &status_for_worker, snapshot);
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, session_id = %session_id, "visual cleanup completed after the session changed"),
            }
        });
    if let Err(error) = spawn {
        let failure = application_error(
            ApplicationErrorCode::WorkerExited,
            format!("Could not start the visual cleanup worker: {error}"),
            ErrorRecoverability::RestartRequired,
            RecoveryAction::RestartApplication,
            Some(session_id),
        );
        fail_and_publish(app, supervisor, status, session_id, failure.clone());
        if let Ok(Some(snapshot)) = supervisor.lock().finish_cleanup(session_id, Err(failure)) {
            publish_visual_runtime(app, status, snapshot);
        }
    }
}

fn take_resources(
    resources: &Arc<Mutex<Option<ActiveVisualResources>>>,
    session_id: SessionId,
) -> Option<ActiveVisualResources> {
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

fn clear_visual_output(
    app: &AppHandle,
    output: &Arc<Mutex<VisualPresentationFrame>>,
    echoes: &Arc<Mutex<Vec<String>>>,
    session_id: SessionId,
    runtime_revision: u32,
) {
    let cleared = {
        let mut current = output.lock();
        let presentation_revision = if current.session_id == session_id {
            current.presentation_revision.saturating_add(1)
        } else {
            0
        };
        let cleared = VisualPresentationFrame {
            session_id,
            runtime_revision,
            presentation_revision,
            source_width: current.source_width.max(1),
            source_height: current.source_height.max(1),
            source_language: current.source_language.clone(),
            target_language: current.target_language.clone(),
            scanning: false,
            regions: Vec::new(),
        };
        *current = cleared.clone();
        cleared
    };
    echoes.lock().clear();
    if let Some(overlay) = app.get_webview_window("visual-overlay") {
        let _ = overlay.emit(ipc::VISUAL_PRESENTATION_EVENT, &cleared);
        let _ = overlay.hide();
    }
    emit_visual_clear(app, session_id, runtime_revision);
}

fn hide_visual_overlay(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window("visual-overlay") {
        let _ = overlay.hide();
    }
}

fn session_is_waiting(
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    session_id: SessionId,
) -> bool {
    let snapshot = supervisor.lock().snapshot();
    snapshot.session_id == Some(session_id) && snapshot.lifecycle == SessionLifecycle::Waiting
}

fn session_is_running(
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    session_id: SessionId,
) -> bool {
    let snapshot = supervisor.lock().snapshot();
    snapshot.session_id == Some(session_id) && snapshot.lifecycle == SessionLifecycle::Running
}

fn resume_visual_if_needed(
    app: &AppHandle,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: &SharedVisualStatus,
    session_id: SessionId,
) {
    if session_is_waiting(supervisor, session_id)
        && let Ok(snapshot) = supervisor.lock().mark_running(session_id)
    {
        publish_visual_runtime(app, status, snapshot);
    }
}

fn mark_visual_waiting(
    app: &AppHandle,
    supervisor: &Arc<Mutex<prollyglot_application_runtime::SessionSupervisor>>,
    status: &SharedVisualStatus,
    session_id: SessionId,
    message: impl Into<String>,
) -> Option<u32> {
    if session_is_waiting(supervisor, session_id) {
        return None;
    }
    let snapshot = supervisor.lock().mark_waiting(session_id, message).ok()?;
    let revision = snapshot.revision;
    publish_visual_runtime(app, status, snapshot);
    Some(revision)
}

fn visual_source_contract(source: prollyglot_visual_windows::VisualSource) -> VisualSource {
    VisualSource {
        id: source.id,
        kind: match source.kind {
            prollyglot_visual_windows::VisualSourceKind::ApplicationWindow => {
                VisualSourceKind::ApplicationWindow
            }
            prollyglot_visual_windows::VisualSourceKind::Display => VisualSourceKind::Display,
        },
        label: source.label,
        x: source.x,
        y: source.y,
        width: source.width,
        height: source.height,
    }
}

fn backend_visual_selection(selection: &VisualCaptureSelection) -> BackendVisualCaptureSelection {
    match selection {
        VisualCaptureSelection::ApplicationWindow { source_id } => {
            BackendVisualCaptureSelection::ApplicationWindow {
                source_id: source_id.clone(),
            }
        }
        VisualCaptureSelection::Display { source_id } => BackendVisualCaptureSelection::Display {
            source_id: source_id.clone(),
        },
        VisualCaptureSelection::Region { display_id, region } => {
            BackendVisualCaptureSelection::Region {
                display_id: display_id.clone(),
                region: prollyglot_visual_pipeline::PixelRect {
                    x: region.x,
                    y: region.y,
                    width: region.width,
                    height: region.height,
                },
            }
        }
    }
}

fn visual_text_contract(
    session_id: SessionId,
    runtime_revision: u32,
    source: &PickedVisualSource,
    update: StabilizerUpdate,
) -> VisualTextUpdate {
    VisualTextUpdate {
        session_id,
        runtime_revision,
        source: VisualCaptureGeometry {
            label: source.label.clone(),
            x: source.x,
            y: source.y,
            width: source.width,
            height: source.height,
        },
        visible: update
            .visible
            .into_iter()
            .map(stable_visual_text_contract)
            .collect(),
        translation_requests: update
            .translation_requests
            .into_iter()
            .map(stable_visual_text_contract)
            .collect(),
        removed_track_ids: update.removed_track_ids,
    }
}

fn stable_visual_text_contract(
    region: prollyglot_visual_pipeline::StableTextRegion,
) -> StableVisualTextRegion {
    StableVisualTextRegion {
        track_id: region.track_id,
        text_revision: region.text_revision,
        text: region.text,
        confidence: region.confidence,
        language: region.language,
        script: region.script,
        bounds: VisualRect {
            x: region.bounds.x,
            y: region.bounds.y,
            width: region.bounds.width,
            height: region.bounds.height,
        },
    }
}

fn visual_session_source(selection: &VisualCaptureSelection) -> SessionSource {
    match selection {
        VisualCaptureSelection::ApplicationWindow { source_id } => SessionSource::new(
            source_id,
            SessionSourceKind::ApplicationWindow,
            "Selected application window",
        ),
        VisualCaptureSelection::Display { source_id } => {
            SessionSource::new(source_id, SessionSourceKind::Display, "Selected display")
        }
        VisualCaptureSelection::Region { display_id, region } => SessionSource::new(
            format!(
                "{display_id}:{}:{}:{}:{}",
                region.x, region.y, region.width, region.height
            ),
            SessionSourceKind::Region,
            "Selected display region",
        ),
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

fn window_operation_error(
    message: impl Into<String>,
    session_id: Option<SessionId>,
) -> ApplicationError {
    application_error(
        ApplicationErrorCode::WindowOperationFailed,
        message,
        ErrorRecoverability::Retryable,
        RecoveryAction::StopAndRetry,
        session_id,
    )
}

fn startup_cancelled(session_id: SessionId) -> ApplicationError {
    application_error(
        ApplicationErrorCode::StartupCancelled,
        "Visual translation startup was cancelled.",
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
    fn overlay_echo_matching_ignores_spacing_and_punctuation() {
        let echoes = vec![normalize_overlay_text("Good morning, everyone!")];
        assert!(matches_overlay_echo("Good morning everyone", &echoes));
        assert!(matches_overlay_echo("Morning, everyone", &echoes));
    }

    #[test]
    fn overlay_echo_matching_does_not_hide_short_source_words() {
        let echoes = vec![normalize_overlay_text("No")];
        assert!(!matches_overlay_echo("No", &echoes));
        assert!(!matches_overlay_echo("News update", &echoes));
    }

    #[test]
    fn visual_presentation_validation_allows_retained_pending_text() {
        let output = VisualPresentationFrame {
            session_id: SessionId(4),
            runtime_revision: 9,
            presentation_revision: 2,
            source_width: 1920,
            source_height: 1080,
            source_language: "ja".into(),
            target_language: "en".into(),
            scanning: false,
            regions: vec![VisualPresentationRegion {
                track_id: 1,
                text_revision: 1,
                original: "日本語".into(),
                translation: None,
                translation_pending: true,
                retained: true,
                bounds: VisualRect {
                    x: 100.0,
                    y: 200.0,
                    width: 300.0,
                    height: 40.0,
                },
            }],
        };

        assert!(validate_visual_presentation(&output).is_ok());
    }

    #[test]
    fn visual_presentation_validation_rejects_inconsistent_translation_state() {
        let output = VisualPresentationFrame {
            session_id: SessionId(4),
            runtime_revision: 9,
            presentation_revision: 2,
            source_width: 1920,
            source_height: 1080,
            source_language: "ja".into(),
            target_language: "en".into(),
            scanning: false,
            regions: vec![VisualPresentationRegion {
                track_id: 1,
                text_revision: 1,
                original: "日本語".into(),
                translation: Some("Japanese".into()),
                translation_pending: true,
                retained: false,
                bounds: VisualRect {
                    x: 100.0,
                    y: 200.0,
                    width: 300.0,
                    height: 40.0,
                },
            }],
        };

        assert!(validate_visual_presentation(&output).is_err());
    }
}
