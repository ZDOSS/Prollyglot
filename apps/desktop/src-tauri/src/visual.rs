use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use prollyglot_model_manager::{
    DEFAULT_VISUAL_OCR_MODEL_ID, DownloadProgress, ModelInstallState, ModelManager, ModelManifest,
    visual_ocr_manifest, visual_ocr_manifest_by_id,
};
use prollyglot_visual_ocr_rapid::RapidOcrEngine;
use prollyglot_visual_pipeline::{
    FrameGate, FrameGateConfig, PixelRect, StabilizerUpdate, TextStabilizer, TextStabilizerConfig,
    VisualPipeline, VisualPipelineStats,
};
use prollyglot_visual_windows::{
    PickedVisualSource, StartedVisualCapture, VisualCaptureCapabilities, VisualCaptureEvent,
    VisualCaptureSelection, VisualSourceSnapshot,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};

use crate::RuntimeState;

const VISUAL_MODEL_STATUS_EVENT: &str = "visual-model-status";
const VISUAL_STATUS_EVENT: &str = "visual-status";
const VISUAL_TEXT_EVENT: &str = "visual-text-update";
const VISUAL_CLEAR_EVENT: &str = "visual-text-clear";
const VISUAL_STATUS_INTERVAL: Duration = Duration::from_millis(500);

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VisualState {
    Starting,
    Capturing,
    Waiting,
    Stopping,
    #[default]
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualStatus {
    pub active: bool,
    pub state: VisualState,
    pub source_label: Option<String>,
    pub frames_received: u64,
    pub frames_analyzed: u64,
    pub frames_unchanged: u64,
    pub replaced_frames: u64,
    pub visible_regions: u64,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisualTextUpdate {
    source: PickedVisualSource,
    #[serde(flatten)]
    update: StabilizerUpdate,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualRegionSelectorRequest {
    display_id: String,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisualRegionSelected {
    display_id: String,
    region: PixelRect,
}

struct ActiveVisualSession {
    capture: StartedVisualCapture,
    capture_events: Option<JoinHandle<()>>,
    processor: Option<JoinHandle<()>>,
}

impl ActiveVisualSession {
    fn stop(&mut self) -> Result<(), String> {
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
            .map_or(Ok(()), Err)
    }
}

#[derive(Default)]
pub struct VisualRuntime {
    catalog: Arc<Mutex<VisualModelCatalogStatus>>,
    installing: Arc<AtomicBool>,
    inspecting: Arc<AtomicBool>,
    status: Arc<Mutex<VisualStatus>>,
    session: Mutex<Option<ActiveVisualSession>>,
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

pub fn is_active(runtime: &VisualRuntime) -> bool {
    runtime.session.lock().is_some()
        || matches!(
            runtime.status.lock().state,
            VisualState::Starting
                | VisualState::Capturing
                | VisualState::Waiting
                | VisualState::Stopping
        )
}

#[tauri::command]
pub fn visual_capabilities() -> VisualCaptureCapabilities {
    prollyglot_visual_windows::capabilities()
}

#[tauri::command]
pub fn visual_source_snapshot() -> Result<VisualSourceSnapshot, String> {
    prollyglot_visual_windows::source_snapshot().map_err(|error| {
        tracing::error!(%error, "could not enumerate visual capture sources");
        error.to_string()
    })
}

#[tauri::command]
pub fn visual_status(state: State<'_, RuntimeState>) -> VisualStatus {
    state.visual.status.lock().clone()
}

#[tauri::command]
pub fn visual_model_status(state: State<'_, RuntimeState>) -> VisualModelCatalogStatus {
    state.visual.catalog.lock().clone()
}

#[tauri::command]
pub fn show_visual_region_selector(
    app: AppHandle,
    display_id: String,
) -> Result<VisualRegionSelectorRequest, String> {
    let display = prollyglot_visual_windows::source_snapshot()
        .map_err(|error| error.to_string())?
        .displays
        .into_iter()
        .find(|display| display.id == display_id)
        .ok_or("The selected display is no longer available.")?;
    let selector = app
        .get_webview_window("region-selector")
        .ok_or("The visual region selector is unavailable.")?;
    selector
        .set_ignore_cursor_events(false)
        .map_err(|error| error.to_string())?;
    selector
        .set_focusable(true)
        .map_err(|error| error.to_string())?;
    selector
        .set_position(PhysicalPosition::new(display.x, display.y))
        .map_err(|error| error.to_string())?;
    selector
        .set_size(PhysicalSize::new(display.width, display.height))
        .map_err(|error| error.to_string())?;
    exclude_window_from_capture(&selector);
    selector.show().map_err(|error| error.to_string())?;
    selector.set_focus().map_err(|error| error.to_string())?;
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
) -> Result<(), String> {
    let display = prollyglot_visual_windows::source_snapshot()
        .map_err(|error| error.to_string())?
        .displays
        .into_iter()
        .find(|display| display.id == display_id)
        .ok_or("The selected display is no longer available.")?;
    if !region.fits_within(display.width, display.height) {
        return Err("The selected region is outside the display.".into());
    }
    if let Some(selector) = app.get_webview_window("region-selector") {
        selector.hide().map_err(|error| error.to_string())?;
    }
    app.emit(
        "visual-region-selected",
        VisualRegionSelected { display_id, region },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn cancel_visual_region_selection(app: AppHandle) -> Result<(), String> {
    if let Some(selector) = app.get_webview_window("region-selector") {
        selector.hide().map_err(|error| error.to_string())?;
    }
    app.emit("visual-region-selection-cancelled", ())
        .map_err(|error| error.to_string())
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
) -> Result<(), String> {
    validate_language_pair(&source_language, &target_language)?;
    {
        let _control = state.control.lock();
        if super::audio_session_active(&state) {
            return Err("Stop audio captions before starting visual translation.".into());
        }
        if is_active(&state.visual) {
            return Err("A visual translation session is already starting or running.".into());
        }
        publish_status(
            &app,
            &state.visual.status,
            VisualStatus {
                state: VisualState::Starting,
                message: Some("Loading local visual text recognition…".into()),
                ..VisualStatus::default()
            },
        );
    }

    let model_directory =
        installed_model_directory(&app, &state.visual).inspect_err(|message| {
            publish_failure(&app, &state.visual.status, message.clone());
        })?;
    let source_language_for_worker = source_language.clone();
    let load_started = Instant::now();
    let engine = tauri::async_runtime::spawn_blocking(move || {
        RapidOcrEngine::load(model_directory, source_language_for_worker)
    })
    .await
    .map_err(|error| {
        let message = format!("Could not join the visual model-loading worker: {error}");
        publish_failure(&app, &state.visual.status, message.clone());
        message
    })?
    .map_err(|error| {
        let message = error.to_string();
        publish_failure(&app, &state.visual.status, message.clone());
        message
    })?;
    tracing::info!(
        elapsed_ms = load_started.elapsed().as_millis(),
        %source_language,
        %target_language,
        "visual OCR model ready"
    );

    let capture = prollyglot_visual_windows::start_capture(selection.clone()).map_err(|error| {
        let message = error.to_string();
        publish_failure(&app, &state.visual.status, message.clone());
        message
    })?;
    let source = Arc::new(Mutex::new(capture.source.clone()));
    configure_visual_overlay(&app, &capture.source).inspect_err(|message| {
        publish_start_failure(&app, &state.visual.status, message.clone());
    })?;

    let processor = spawn_processor(
        app.clone(),
        Arc::clone(&state.visual.status),
        Arc::clone(&source),
        capture.frames.clone(),
        engine,
    )
    .inspect_err(|message| {
        publish_start_failure(&app, &state.visual.status, message.clone());
    })?;
    let capture_events = match spawn_capture_events(
        app.clone(),
        Arc::clone(&state.visual.status),
        Arc::clone(&source),
        capture.events.clone(),
    ) {
        Ok(worker) => worker,
        Err(error) => {
            drop(processor);
            publish_start_failure(&app, &state.visual.status, error.clone());
            return Err(error);
        }
    };

    let source_label = capture.source.label.clone();
    *state.visual.session.lock() = Some(ActiveVisualSession {
        capture,
        capture_events: Some(capture_events),
        processor: Some(processor),
    });
    publish_status(
        &app,
        &state.visual.status,
        VisualStatus {
            active: true,
            state: VisualState::Capturing,
            source_label: Some(source_label),
            message: Some(format!(
                "Watching the live source, recognizing {source_language} text, and translating to {target_language}."
            )),
            ..VisualStatus::default()
        },
    );
    Ok(())
}

#[tauri::command]
pub fn stop_visual_translation(
    app: AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<(), String> {
    let _control = state.control.lock();
    let Some(mut session) = state.visual.session.lock().take() else {
        return Err("No visual translation session is running.".into());
    };
    let previous = state.visual.status.lock().clone();
    publish_status(
        &app,
        &state.visual.status,
        VisualStatus {
            active: true,
            state: VisualState::Stopping,
            message: None,
            ..previous
        },
    );
    let stop_result = session.stop();
    if let Some(overlay) = app.get_webview_window("visual-overlay") {
        let _ = overlay.hide();
    }
    let _ = app.emit(VISUAL_CLEAR_EVENT, ());
    match stop_result {
        Ok(()) => {
            tracing::info!("visual translation session stopped");
            publish_status(&app, &state.visual.status, VisualStatus::default());
            Ok(())
        }
        Err(message) => {
            publish_status(
                &app,
                &state.visual.status,
                VisualStatus {
                    active: false,
                    state: VisualState::Failed,
                    message: Some(message.clone()),
                    ..VisualStatus::default()
                },
            );
            Err(message)
        }
    }
}

fn spawn_processor(
    app: AppHandle,
    status: Arc<Mutex<VisualStatus>>,
    source: Arc<Mutex<PickedVisualSource>>,
    frames: crossbeam_channel::Receiver<prollyglot_visual_pipeline::VisualFrame>,
    engine: RapidOcrEngine,
) -> Result<JoinHandle<()>, String> {
    thread::Builder::new()
        .name("visual-ocr".into())
        .spawn(move || {
            let mut pipeline = VisualPipeline::new(
                FrameGate::new(FrameGateConfig::default()),
                engine,
                TextStabilizer::new(TextStabilizerConfig::default()),
            );
            let mut last_status_publish = Instant::now()
                .checked_sub(VISUAL_STATUS_INTERVAL)
                .unwrap_or_else(Instant::now);
            while let Ok(frame) = frames.recv() {
                let outcome = match pipeline.process(&frame) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        let message = error.to_string();
                        tracing::error!(%error, "visual OCR inference failed");
                        publish_failure(&app, &status, message);
                        break;
                    }
                };
                if let Some(update) = outcome.update {
                    let payload = VisualTextUpdate {
                        source: source.lock().clone(),
                        update,
                    };
                    if let Err(error) = app.emit(VISUAL_TEXT_EVENT, payload) {
                        tracing::warn!(%error, "could not emit visual text update");
                    }
                }
                if last_status_publish.elapsed() >= VISUAL_STATUS_INTERVAL {
                    publish_pipeline_stats(&app, &status, outcome.stats);
                    last_status_publish = Instant::now();
                }
            }
        })
        .map_err(|error| format!("Could not start the visual OCR worker: {error}"))
}

fn spawn_capture_events(
    app: AppHandle,
    status: Arc<Mutex<VisualStatus>>,
    source: Arc<Mutex<PickedVisualSource>>,
    events: crossbeam_channel::Receiver<VisualCaptureEvent>,
) -> Result<JoinHandle<()>, String> {
    thread::Builder::new()
        .name("visual-capture-events".into())
        .spawn(move || {
            while let Ok(event) = events.recv() {
                match event {
                    VisualCaptureEvent::Started(next_source) => {
                        *source.lock() = next_source.clone();
                        if let Err(error) = configure_visual_overlay(&app, &next_source) {
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
                            if let Err(error) = configure_visual_overlay(&app, &next_source) {
                                tracing::warn!(%error, "could not follow visual source geometry");
                            }
                        }
                        let mut next = status.lock().clone();
                        next.replaced_frames = replaced_frames;
                        publish_status(&app, &status, next);
                    }
                    VisualCaptureEvent::SourceClosed => {
                        let message = "The selected visual source closed. Stop visual translation or choose another source.".to_owned();
                        tracing::warn!(%message, "visual source closed");
                        let mut next = status.lock().clone();
                        next.state = VisualState::Waiting;
                        next.message = Some(message);
                        publish_status(&app, &status, next);
                        if let Some(overlay) = app.get_webview_window("visual-overlay") {
                            let _ = overlay.hide();
                        }
                        break;
                    }
                }
            }
        })
        .map_err(|error| format!("Could not start the visual capture event worker: {error}"))
}

fn publish_pipeline_stats(
    app: &AppHandle,
    status: &Arc<Mutex<VisualStatus>>,
    stats: VisualPipelineStats,
) {
    let mut next = status.lock().clone();
    next.frames_received = stats.frames_received;
    next.frames_analyzed = stats.frames_analyzed;
    next.frames_unchanged = stats.frames_unchanged;
    next.visible_regions = stats.stable_regions;
    publish_status(app, status, next);
}

fn configure_visual_overlay(app: &AppHandle, source: &PickedVisualSource) -> Result<(), String> {
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
        .set_position(PhysicalPosition::new(source.x, source.y))
        .map_err(|error| error.to_string())?;
    overlay
        .set_size(PhysicalSize::new(source.width, source.height))
        .map_err(|error| error.to_string())?;
    exclude_window_from_capture(&overlay);
    overlay.show().map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
pub fn exclude_window_from_capture(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
    };

    match window.hwnd() {
        Ok(hwnd) => {
            let hwnd = HWND(hwnd.0);
            if let Err(error) = unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) } {
                tracing::warn!(window = window.label(), %error, "could not exclude Prollyglot window from display capture");
            }
        }
        Err(error) => {
            tracing::warn!(window = window.label(), %error, "could not read Prollyglot window handle for capture exclusion");
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn exclude_window_from_capture(_window: &WebviewWindow) {}

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
    if is_active(&state.visual) || super::audio_session_active(state) {
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

fn publish_status(app: &AppHandle, status: &Arc<Mutex<VisualStatus>>, next: VisualStatus) {
    *status.lock() = next.clone();
    if let Err(error) = app.emit(VISUAL_STATUS_EVENT, next) {
        tracing::warn!(%error, "could not emit visual translation status");
    }
}

fn publish_failure(app: &AppHandle, status: &Arc<Mutex<VisualStatus>>, message: String) {
    tracing::error!(%message, "visual translation failed");
    let previous = status.lock().clone();
    publish_status(
        app,
        status,
        VisualStatus {
            state: VisualState::Failed,
            message: Some(message),
            ..previous
        },
    );
}

fn publish_start_failure(app: &AppHandle, status: &Arc<Mutex<VisualStatus>>, message: String) {
    if let Some(overlay) = app.get_webview_window("visual-overlay") {
        let _ = overlay.hide();
    }
    let _ = app.emit(VISUAL_CLEAR_EVENT, ());
    publish_failure(app, status, message);
}
