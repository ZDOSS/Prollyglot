use std::{
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
    DownloadProgress, ModelInstallState, ModelManager, initial_english_manifest,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::RuntimeState;

const MODEL_STATUS_EVENT: &str = "model-status";

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
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub message: Option<String>,
}

impl Default for ModelStatus {
    fn default() -> Self {
        manifest_status(ModelPhase::NotInstalled, 0, None).unwrap_or_else(|error| Self {
            phase: ModelPhase::Failed,
            model_id: "initial-english".into(),
            display_name: "English streaming model".into(),
            downloaded_bytes: 0,
            total_bytes: 0,
            message: Some(error),
        })
    }
}

#[derive(Default)]
pub struct ModelRuntime {
    pub status: Arc<Mutex<ModelStatus>>,
    pub installing: Arc<AtomicBool>,
}

pub fn models_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|directory| directory.join("models"))
        .map_err(|error| format!("Could not resolve the local model directory: {error}"))
}

pub fn initialize(app: &AppHandle, runtime: &ModelRuntime) {
    let next = inspect(app).unwrap_or_else(|message| {
        manifest_status(ModelPhase::Failed, 0, Some(message.clone())).unwrap_or(ModelStatus {
            phase: ModelPhase::Failed,
            model_id: "initial-english".into(),
            display_name: "English streaming model".into(),
            downloaded_bytes: 0,
            total_bytes: 0,
            message: Some(message),
        })
    });
    *runtime.status.lock() = next;
}

#[tauri::command]
pub fn model_status(state: State<'_, RuntimeState>) -> ModelStatus {
    state.model.status.lock().clone()
}

#[tauri::command]
pub fn install_english_model(app: AppHandle, state: State<'_, RuntimeState>) -> Result<(), String> {
    let manifest = initial_english_manifest().map_err(|error| error.to_string())?;
    let root = models_root(&app)?;
    state
        .model
        .installing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "The English model is already downloading.".to_owned())?;

    let total_bytes = manifest.download_size_bytes();
    publish(
        &app,
        &state.model.status,
        ModelStatus {
            phase: ModelPhase::Downloading,
            model_id: manifest.id.clone(),
            display_name: manifest.display_name.clone(),
            downloaded_bytes: 0,
            total_bytes,
            message: Some("Downloading and verifying the local English model…".into()),
        },
    );

    let status = Arc::clone(&state.model.status);
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
                    publish_progress(&app_for_worker, &status, &manifest.display_name, progress);
                    last_publish = Instant::now();
                }
            });
            let next = match result {
                Ok(_) => ModelStatus {
                    phase: ModelPhase::Ready,
                    model_id: manifest.id.clone(),
                    display_name: manifest.display_name.clone(),
                    downloaded_bytes: total_bytes,
                    total_bytes,
                    message: None,
                },
                Err(error) => {
                    tracing::error!(%error, "English model installation failed");
                    ModelStatus {
                        phase: ModelPhase::Failed,
                        model_id: manifest.id.clone(),
                        display_name: manifest.display_name.clone(),
                        downloaded_bytes: 0,
                        total_bytes,
                        message: Some(error.to_string()),
                    }
                }
            };
            publish(&app_for_worker, &status, next);
            installing.store(false, Ordering::Release);
        });
    if let Err(error) = spawn_result {
        state.model.installing.store(false, Ordering::Release);
        let message = format!("Could not start the model download worker: {error}");
        publish(
            &app,
            &state.model.status,
            manifest_status(ModelPhase::Failed, 0, Some(message.clone()))?,
        );
        return Err(message);
    }
    Ok(())
}

#[tauri::command]
pub fn remove_english_model(app: AppHandle, state: State<'_, RuntimeState>) -> Result<(), String> {
    let _control = state.control.lock();
    if matches!(
        state.status.lock().state,
        CaptureState::Starting
            | CaptureState::Capturing
            | CaptureState::Waiting
            | CaptureState::Stopping
    ) || state.session.lock().is_some()
    {
        return Err("Stop captions before removing the active English model.".into());
    }
    if state.model.installing.load(Ordering::Acquire) {
        return Err("Wait for the current model download to finish before removing it.".into());
    }
    let manifest = initial_english_manifest().map_err(|error| error.to_string())?;
    ModelManager::new(models_root(&app)?)
        .remove(&manifest)
        .map_err(|error| error.to_string())?;
    publish(
        &app,
        &state.model.status,
        manifest_status(ModelPhase::NotInstalled, 0, None)?,
    );
    Ok(())
}

fn inspect(app: &AppHandle) -> Result<ModelStatus, String> {
    let manifest = initial_english_manifest().map_err(|error| error.to_string())?;
    let manager = ModelManager::new(models_root(app)?);
    let state = manager
        .state(&manifest)
        .map_err(|error| error.to_string())?;
    match state {
        ModelInstallState::NotInstalled => manifest_status(ModelPhase::NotInstalled, 0, None),
        ModelInstallState::Ready => {
            manifest_status(ModelPhase::Ready, manifest.download_size_bytes(), None)
        }
        ModelInstallState::Corrupt { issues } => manifest_status(
            ModelPhase::Corrupt,
            0,
            Some(format!(
                "The installed model is incomplete or corrupt: {}",
                issues.join("; ")
            )),
        ),
    }
}

fn manifest_status(
    phase: ModelPhase,
    downloaded_bytes: u64,
    message: Option<String>,
) -> Result<ModelStatus, String> {
    let manifest = initial_english_manifest().map_err(|error| error.to_string())?;
    let total_bytes = manifest.download_size_bytes();
    Ok(ModelStatus {
        phase,
        model_id: manifest.id,
        display_name: manifest.display_name,
        downloaded_bytes,
        total_bytes,
        message,
    })
}

fn publish_progress(
    app: &AppHandle,
    status: &Arc<Mutex<ModelStatus>>,
    display_name: &str,
    progress: DownloadProgress,
) {
    publish(
        app,
        status,
        ModelStatus {
            phase: ModelPhase::Downloading,
            model_id: progress.model_id,
            display_name: display_name.into(),
            downloaded_bytes: progress.completed_bytes,
            total_bytes: progress.model_bytes,
            message: Some(format!("Downloading {}…", progress.artifact_role)),
        },
    );
}

fn publish(app: &AppHandle, status: &Arc<Mutex<ModelStatus>>, next: ModelStatus) {
    *status.lock() = next.clone();
    if let Err(error) = app.emit(MODEL_STATUS_EVENT, next) {
        tracing::warn!(%error, "could not emit model status");
    }
}
