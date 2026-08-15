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
use prollyglot_model_manager::{
    DEFAULT_SPEECH_MODEL_ID, DownloadProgress, ModelInstallState, ModelManager, ModelManifest,
    NEMOTRON_MULTILINGUAL_MODEL_ID, speech_manifest, speech_model_manifests,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{RuntimeState, configuration::ConfigurationRuntime};

const MODEL_STATUS_EVENT: &str = "model-status";
const SELECTED_MODEL_FILE: &str = "selected-speech-model.txt";
const LEGACY_SELECTED_MODEL_FILE: &str = "selected-english-model.txt";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelPhase {
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
pub struct ModelStatus {
    pub phase: ModelPhase,
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
pub struct ModelCatalogStatus {
    pub selected_model_id: String,
    pub models: Vec<ModelStatus>,
}

impl Default for ModelCatalogStatus {
    fn default() -> Self {
        match speech_model_manifests() {
            Ok(manifests) => Self {
                selected_model_id: DEFAULT_SPEECH_MODEL_ID.into(),
                models: manifests
                    .iter()
                    .map(|manifest| {
                        status_for_manifest(manifest, ModelPhase::NotInstalled, 0, None)
                    })
                    .collect(),
            },
            Err(error) => Self {
                selected_model_id: DEFAULT_SPEECH_MODEL_ID.into(),
                models: vec![ModelStatus {
                    phase: ModelPhase::Failed,
                    model_id: DEFAULT_SPEECH_MODEL_ID.into(),
                    display_name: "Streaming speech model".into(),
                    profile: "Fast".into(),
                    description: "Local streaming captions.".into(),
                    languages: vec!["en".into()],
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
    inspecting: Arc<AtomicBool>,
}

pub fn models_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|directory| directory.join("models"))
        .map_err(|error| format!("Could not resolve the local model directory: {error}"))
}

pub fn initialize(app: &AppHandle, runtime: &ModelRuntime, configuration: &ConfigurationRuntime) {
    let selected_model_id = resolve_selected_model(app, configuration);
    let checking = speech_model_manifests()
        .map(|manifests| ModelCatalogStatus {
            selected_model_id: selected_model_id.clone(),
            models: manifests
                .iter()
                .map(|manifest| {
                    status_for_manifest(
                        manifest,
                        ModelPhase::Checking,
                        0,
                        Some("Checking local model files…".into()),
                    )
                })
                .collect(),
        })
        .unwrap_or_else(|error| {
            inspection_failure_catalog(selected_model_id.clone(), error.to_string())
        });
    *runtime.catalog.lock() = checking;
    runtime.inspecting.store(true, Ordering::Release);

    let app_for_worker = app.clone();
    let catalog = Arc::clone(&runtime.catalog);
    let inspecting = Arc::clone(&runtime.inspecting);
    let selected_for_worker = selected_model_id.clone();
    let spawn_result = thread::Builder::new()
        .name("model-inspection".into())
        .spawn(move || {
            let started = Instant::now();
            let next = inspect(&app_for_worker, selected_for_worker.clone())
                .unwrap_or_else(|message| inspection_failure_catalog(selected_for_worker, message));
            let installed_models = next
                .models
                .iter()
                .filter(|model| model.phase == ModelPhase::Ready)
                .count();
            *catalog.lock() = next.clone();
            inspecting.store(false, Ordering::Release);
            tracing::info!(
                elapsed_ms = started.elapsed().as_millis(),
                installed_models,
                "speech model catalog inspection completed"
            );
            publish(&app_for_worker, next);
        });
    if let Err(error) = spawn_result {
        runtime.inspecting.store(false, Ordering::Release);
        let next = inspection_failure_catalog(
            selected_model_id,
            format!("Could not start the model inspection worker: {error}"),
        );
        *runtime.catalog.lock() = next.clone();
        publish(app, next);
    }
}

fn inspection_failure_catalog(selected_model_id: String, message: String) -> ModelCatalogStatus {
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
}

pub fn selected_model_id(runtime: &ModelRuntime, language: &str) -> Result<String, String> {
    let catalog = runtime.catalog.lock();
    let selected = catalog
        .models
        .iter()
        .find(|model| model.model_id == catalog.selected_model_id)
        .ok_or("The selected speech model is unavailable.")?;
    if selected.phase != ModelPhase::Ready {
        return Err(format!(
            "Install {} before starting captions.",
            selected.display_name
        ));
    }
    if !selected
        .languages
        .iter()
        .any(|candidate| candidate == language)
    {
        return Err(format!(
            "{} does not support the selected spoken language.",
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
pub fn select_speech_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    model_id: String,
) -> Result<(), String> {
    let manifest = speech_manifest(&model_id).map_err(|error| error.to_string())?;
    let _control = state.control.lock();
    require_stopped(&state)?;
    require_catalog_ready(&state)?;
    let accepted =
        crate::configuration::set_speech_model(&app, &state.configuration, manifest.id.clone())?;
    if accepted.config.models.speech_model_id.as_deref() != Some(manifest.id.as_str()) {
        return Err("The selected speech model was not accepted by local configuration.".into());
    }
    remove_legacy_selected_model_files(&app);

    let next = {
        let mut catalog = state.model.catalog.lock();
        catalog.selected_model_id = manifest.id;
        catalog.clone()
    };
    publish(&app, next);
    Ok(())
}

#[tauri::command]
pub fn install_speech_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    model_id: String,
) -> Result<(), String> {
    let manifest = speech_manifest(&model_id).map_err(|error| error.to_string())?;
    require_catalog_ready(&state)?;
    let root = models_root(&app)?;
    state
        .model
        .installing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "Another speech model is already downloading.".to_owned())?;

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
                    tracing::error!(model_id = %manifest.id, %error, "speech model installation failed");
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
pub fn remove_speech_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    model_id: String,
) -> Result<(), String> {
    let manifest = speech_manifest(&model_id).map_err(|error| error.to_string())?;
    let _control = state.control.lock();
    require_stopped(&state)?;
    require_catalog_ready(&state)?;
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
    if crate::audio::is_active(state) || crate::visual::is_active(state) {
        Err("Stop captions and visual translation before changing local speech models.".into())
    } else {
        Ok(())
    }
}

fn require_catalog_ready(state: &RuntimeState) -> Result<(), String> {
    if state.model.inspecting.load(Ordering::Acquire) {
        Err("Wait for Prollyglot to finish checking the installed speech models.".into())
    } else {
        Ok(())
    }
}

fn inspect(app: &AppHandle, selected_model_id: String) -> Result<ModelCatalogStatus, String> {
    let manifests = speech_model_manifests().map_err(|error| error.to_string())?;
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
        languages: manifest.languages.clone(),
        downloaded_bytes,
        total_bytes: manifest.download_size_bytes(),
        message,
    }
}

fn product_metadata(model_id: &str) -> (&'static str, &'static str) {
    match model_id {
        DEFAULT_SPEECH_MODEL_ID => (
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
        "sherpa-zipformer-zh-14m-2023-02-23" => (
            "Chinese · Small",
            "A low-footprint 14M streaming model for responsive Mandarin captions.",
        ),
        "sherpa-zipformer-fr-2023-04-14" => (
            "French · Compact",
            "A dedicated streaming French model with much lower resource use than Nemotron.",
        ),
        "sherpa-zipformer-ko-2024-06-16" => (
            "Korean · Compact",
            "A dedicated streaming Korean model with lower resource use than Nemotron.",
        ),
        "sherpa-zipformer-bn-vosk-2026-02-09" => (
            "Bengali · Compact",
            "A dedicated streaming Bengali model for local, lower-resource captions.",
        ),
        NEMOTRON_MULTILINGUAL_MODEL_ID => (
            "Multilingual",
            "A high-resource 600M-parameter CPU model covering 28 languages plus automatic detection. Expect about 1 GB of app memory; broad-coverage languages and automatic detection may be less accurate.",
        ),
        _ => ("Speech", "Local streaming captions."),
    }
}

fn resolve_selected_model(app: &AppHandle, configuration: &ConfigurationRuntime) -> String {
    match configuration.snapshot() {
        Ok(snapshot) => {
            if let Some(model_id) = snapshot.config.models.speech_model_id {
                match speech_manifest(&model_id) {
                    Ok(_) => {
                        remove_legacy_selected_model_files(app);
                        return model_id;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "resetting an unknown configured speech model");
                    }
                }
            }
        }
        Err(error) => {
            tracing::warn!(%error, "could not read the selected speech model from configuration")
        }
    }

    let model_id =
        read_legacy_selected_model(app).unwrap_or_else(|| DEFAULT_SPEECH_MODEL_ID.into());
    match crate::configuration::set_speech_model(app, configuration, model_id.clone()) {
        Ok(snapshot)
            if snapshot.config.models.speech_model_id.as_deref() == Some(model_id.as_str()) =>
        {
            remove_legacy_selected_model_files(app);
        }
        Ok(_) => tracing::warn!("the selected speech model did not survive configuration readback"),
        Err(error) => {
            tracing::warn!(%error, "could not migrate the selected speech model into configuration")
        }
    }
    model_id
}

fn read_legacy_selected_model(app: &AppHandle) -> Option<String> {
    let paths = [
        (SELECTED_MODEL_FILE, false),
        (LEGACY_SELECTED_MODEL_FILE, true),
    ];
    for (file_name, legacy) in paths {
        let path = match selected_model_path_for(app, file_name) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(%error, "could not resolve the selected-model preference");
                return None;
            }
        };
        let value = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "could not read the selected-model preference");
                return None;
            }
        };
        let model_id = value.trim();
        match speech_manifest(model_id) {
            Ok(_) => {
                if legacy {
                    tracing::info!("found the legacy English speech-model preference");
                }
                return Some(model_id.into());
            }
            Err(error) => {
                tracing::warn!(%error, "ignoring an unknown selected-model preference");
            }
        }
    }
    None
}

fn remove_legacy_selected_model_files(app: &AppHandle) {
    for file_name in [SELECTED_MODEL_FILE, LEGACY_SELECTED_MODEL_FILE] {
        let path = match selected_model_path_for(app, file_name) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(%error, "could not resolve a legacy selected-model preference");
                continue;
            }
        };
        match fs::remove_file(&path) {
            Ok(()) => {
                tracing::info!(path = %path.display(), "removed migrated selected-model preference")
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "could not remove migrated selected-model preference")
            }
        }
    }
}

fn selected_model_path_for(app: &AppHandle, file_name: &str) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|directory| directory.join(file_name))
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
