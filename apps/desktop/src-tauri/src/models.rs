use std::{
    fs, io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use prollyglot_core::CaptureState;
use prollyglot_model_manager::{
    DEFAULT_ENGLISH_MODEL_ID, DownloadProgress, ModelInstallState, ModelManager, ModelManifest,
    english_manifest, english_model_manifests,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::RuntimeState;

const MODEL_STATUS_EVENT: &str = "model-status";
const SELECTED_MODEL_FILE: &str = "selected-english-model.txt";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelPhase {
    #[default]
    NotInstalled,
    Downloading,
    Ready,
    Corrupt,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub phase: ModelPhase,
    pub model_id: String,
    pub display_name: String,
    pub profile: String,
    pub description: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogStatus {
    pub selected_model_id: String,
    pub models: Vec<ModelStatus>,
}

impl Default for ModelCatalogStatus {
    fn default() -> Self {
        match english_model_manifests() {
            Ok(manifests) => Self {
                selected_model_id: DEFAULT_ENGLISH_MODEL_ID.into(),
                models: manifests
                    .iter()
                    .map(|manifest| {
                        status_for_manifest(manifest, ModelPhase::NotInstalled, 0, None)
                    })
                    .collect(),
            },
            Err(error) => Self {
                selected_model_id: DEFAULT_ENGLISH_MODEL_ID.into(),
                models: vec![ModelStatus {
                    phase: ModelPhase::Failed,
                    model_id: DEFAULT_ENGLISH_MODEL_ID.into(),
                    display_name: "English streaming model".into(),
                    profile: "Fast".into(),
                    description: "Local streaming English captions.".into(),
                    downloaded_bytes: 0,
                    total_bytes: 0,
                    message: Some(error.to_string()),
                }],
            },
        }
    }
}

#[derive(Default)]
pub struct ModelRuntime {
    catalog: Arc<Mutex<ModelCatalogStatus>>,
    installing: Arc<AtomicBool>,
}

pub fn models_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|directory| directory.join("models"))
        .map_err(|error| format!("Could not resolve the local model directory: {error}"))
}

pub fn initialize(app: &AppHandle, runtime: &ModelRuntime) {
    let selected_model_id = read_selected_model(app);
    let next = inspect(app, selected_model_id.clone()).unwrap_or_else(|message| {
        let mut fallback = ModelCatalogStatus {
            selected_model_id,
            ..ModelCatalogStatus::default()
        };
        if let Some(selected) = fallback
            .models
            .iter_mut()
            .find(|model| model.model_id == fallback.selected_model_id)
        {
            selected.phase = ModelPhase::Failed;
            selected.message = Some(message);
        }
        fallback
    });
    *runtime.catalog.lock() = next;
}

pub fn selected_model_id(runtime: &ModelRuntime) -> Result<String, String> {
    let catalog = runtime.catalog.lock();
    let selected = catalog
        .models
        .iter()
        .find(|model| model.model_id == catalog.selected_model_id)
        .ok_or("The selected English model is unavailable.")?;
    if selected.phase != ModelPhase::Ready {
        return Err(format!(
            "Install {} before starting captions.",
            selected.display_name
        ));
    }
    Ok(selected.model_id.clone())
}

#[tauri::command]
pub fn model_status(state: State<'_, RuntimeState>) -> ModelCatalogStatus {
    state.model.catalog.lock().clone()
}

#[tauri::command]
pub fn select_english_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    model_id: String,
) -> Result<(), String> {
    let manifest = english_manifest(&model_id).map_err(|error| error.to_string())?;
    let _control = state.control.lock();
    require_stopped(&state)?;
    persist_selected_model(&app, &manifest.id)?;

    let next = {
        let mut catalog = state.model.catalog.lock();
        catalog.selected_model_id = manifest.id;
        catalog.clone()
    };
    publish(&app, next);
    Ok(())
}

#[tauri::command]
pub fn install_english_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    model_id: String,
) -> Result<(), String> {
    let manifest = english_manifest(&model_id).map_err(|error| error.to_string())?;
    let root = models_root(&app)?;
    state
        .model
        .installing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "Another English model is already downloading.".to_owned())?;

    update_model(
        &app,
        &state.model.catalog,
        &manifest.id,
        ModelPhase::Downloading,
        0,
        Some(format!(
            "Downloading and verifying {}…",
            manifest.display_name
        )),
    );

    let catalog = Arc::clone(&state.model.catalog);
    let installing = Arc::clone(&state.model.installing);
    let app_for_worker = app.clone();
    let spawn_result = thread::Builder::new()
        .name("model-download".into())
        .spawn(move || {
            let manager = ModelManager::new(root);
            let mut last_publish = Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now);
            let result = manager.install(&manifest, |progress| {
                if last_publish.elapsed() >= Duration::from_millis(100)
                    || progress.completed_bytes == progress.model_bytes
                {
                    publish_progress(&app_for_worker, &catalog, progress);
                    last_publish = Instant::now();
                }
            });
            match result {
                Ok(_) => update_model(
                    &app_for_worker,
                    &catalog,
                    &manifest.id,
                    ModelPhase::Ready,
                    manifest.download_size_bytes(),
                    None,
                ),
                Err(error) => {
                    tracing::error!(model_id = %manifest.id, %error, "English model installation failed");
                    update_model(
                        &app_for_worker,
                        &catalog,
                        &manifest.id,
                        ModelPhase::Failed,
                        0,
                        Some(error.to_string()),
                    );
                }
            }
            installing.store(false, Ordering::Release);
        });
    if let Err(error) = spawn_result {
        state.model.installing.store(false, Ordering::Release);
        let message = format!("Could not start the model download worker: {error}");
        update_model(
            &app,
            &state.model.catalog,
            &model_id,
            ModelPhase::Failed,
            0,
            Some(message.clone()),
        );
        return Err(message);
    }
    Ok(())
}

#[tauri::command]
pub fn remove_english_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    model_id: String,
) -> Result<(), String> {
    let manifest = english_manifest(&model_id).map_err(|error| error.to_string())?;
    let _control = state.control.lock();
    require_stopped(&state)?;
    if state.model.installing.load(Ordering::Acquire) {
        return Err(
            "Wait for the current model download to finish before removing a model.".into(),
        );
    }
    ModelManager::new(models_root(&app)?)
        .remove(&manifest)
        .map_err(|error| error.to_string())?;
    update_model(
        &app,
        &state.model.catalog,
        &manifest.id,
        ModelPhase::NotInstalled,
        0,
        None,
    );
    Ok(())
}

fn require_stopped(state: &RuntimeState) -> Result<(), String> {
    if matches!(
        state.status.lock().state,
        CaptureState::Starting
            | CaptureState::Capturing
            | CaptureState::Waiting
            | CaptureState::Stopping
    ) || state.session.lock().is_some()
    {
        Err("Stop captions before changing local speech models.".into())
    } else {
        Ok(())
    }
}

fn inspect(app: &AppHandle, selected_model_id: String) -> Result<ModelCatalogStatus, String> {
    let manifests = english_model_manifests().map_err(|error| error.to_string())?;
    let manager = ModelManager::new(models_root(app)?);
    let models = manifests
        .iter()
        .map(|manifest| match manager.state(manifest) {
            Ok(ModelInstallState::NotInstalled) => {
                status_for_manifest(manifest, ModelPhase::NotInstalled, 0, None)
            }
            Ok(ModelInstallState::Ready) => status_for_manifest(
                manifest,
                ModelPhase::Ready,
                manifest.download_size_bytes(),
                None,
            ),
            Ok(ModelInstallState::Corrupt { issues }) => status_for_manifest(
                manifest,
                ModelPhase::Corrupt,
                0,
                Some(format!(
                    "The installed model is incomplete or corrupt: {}",
                    issues.join("; ")
                )),
            ),
            Err(error) => {
                status_for_manifest(manifest, ModelPhase::Failed, 0, Some(error.to_string()))
            }
        })
        .collect();
    Ok(ModelCatalogStatus {
        selected_model_id,
        models,
    })
}

fn status_for_manifest(
    manifest: &ModelManifest,
    phase: ModelPhase,
    downloaded_bytes: u64,
    message: Option<String>,
) -> ModelStatus {
    let (profile, description) = product_metadata(&manifest.id);
    ModelStatus {
        phase,
        model_id: manifest.id.clone(),
        display_name: manifest.display_name.clone(),
        profile: profile.into(),
        description: description.into(),
        downloaded_bytes,
        total_bytes: manifest.download_size_bytes(),
        message,
    }
}

fn product_metadata(model_id: &str) -> (&'static str, &'static str) {
    match model_id {
        DEFAULT_ENGLISH_MODEL_ID => (
            "Fast",
            "Lowest download and CPU cost for responsive captions on ordinary PCs.",
        ),
        "sherpa-zipformer-en-standard-2023-06-26" => (
            "Balanced",
            "A larger streaming model with more capacity while remaining comfortably real-time in local tests.",
        ),
        "sherpa-zipformer-en-gigaspeech-2023-06-21" => (
            "Enhanced",
            "The broadest English option, trained on LibriSpeech and GigaSpeech for a better chance on varied speech.",
        ),
        _ => ("English", "Local streaming English captions."),
    }
}

fn read_selected_model(app: &AppHandle) -> String {
    let path = match selected_model_path(app) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(%error, "could not resolve the selected-model preference");
            return DEFAULT_ENGLISH_MODEL_ID.into();
        }
    };
    let value = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return DEFAULT_ENGLISH_MODEL_ID.into();
        }
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "could not read the selected-model preference");
            return DEFAULT_ENGLISH_MODEL_ID.into();
        }
    };
    let model_id = value.trim();
    match english_manifest(model_id) {
        Ok(_) => model_id.into(),
        Err(error) => {
            tracing::warn!(%error, "ignoring an unknown selected-model preference");
            DEFAULT_ENGLISH_MODEL_ID.into()
        }
    }
}

fn persist_selected_model(app: &AppHandle, model_id: &str) -> Result<(), String> {
    let path = selected_model_path(app)?;
    let parent = path
        .parent()
        .ok_or("The selected-model preference path is invalid.")?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Could not create the local preferences directory {}: {error}",
            parent.display()
        )
    })?;
    fs::write(&path, format!("{model_id}\n")).map_err(|error| {
        format!(
            "Could not save the selected speech model to {}: {error}",
            path.display()
        )
    })
}

fn selected_model_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|directory| directory.join(SELECTED_MODEL_FILE))
        .map_err(|error| format!("Could not resolve the local preferences directory: {error}"))
}

fn publish_progress(
    app: &AppHandle,
    catalog: &Arc<Mutex<ModelCatalogStatus>>,
    progress: DownloadProgress,
) {
    update_model(
        app,
        catalog,
        &progress.model_id,
        ModelPhase::Downloading,
        progress.completed_bytes,
        Some(format!("Downloading {}…", progress.artifact_role)),
    );
}

fn update_model(
    app: &AppHandle,
    catalog: &Arc<Mutex<ModelCatalogStatus>>,
    model_id: &str,
    phase: ModelPhase,
    downloaded_bytes: u64,
    message: Option<String>,
) {
    let next = {
        let mut catalog = catalog.lock();
        let Some(model) = catalog
            .models
            .iter_mut()
            .find(|model| model.model_id == model_id)
        else {
            tracing::error!(%model_id, "could not update an unknown model");
            return;
        };
        model.phase = phase;
        model.downloaded_bytes = downloaded_bytes.min(model.total_bytes);
        model.message = message;
        catalog.clone()
    };
    publish(app, next);
}

fn publish(app: &AppHandle, next: ModelCatalogStatus) {
    if let Err(error) = app.emit(MODEL_STATUS_EVENT, next) {
        tracing::warn!(%error, "could not emit model status");
    }
}
