use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use prollyglot_model_manager::{
    DownloadProgress, ModelInstallState, ModelManager, ModelManagerError, ModelManifest,
    translation_manifest, translation_model_manifests,
};
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, State,
    http::{Request, Response, StatusCode, header},
};

use crate::{RuntimeState, models::models_root};

const TRANSLATION_MODEL_STATUS_EVENT: &str = "translation-model-status";
const MODEL_PROTOCOL_PREFIX: &str = "translation";
const VERIFIED_RESOURCE: &str = "verified";
const MAX_PROTOCOL_CHUNK_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TranslationModelPhase {
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
pub struct TranslationStorageStatus {
    pub phase: TranslationModelPhase,
    pub storage_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationStorageCatalog {
    pub models: Vec<TranslationStorageStatus>,
}

#[derive(Default)]
pub struct TranslationRuntime {
    catalog: Arc<Mutex<TranslationStorageCatalog>>,
    installing: Arc<AtomicBool>,
    inspecting: Arc<AtomicBool>,
}

pub fn initialize(app: &AppHandle, runtime: &TranslationRuntime) {
    let checking = translation_model_manifests()
        .map(|manifests| TranslationStorageCatalog {
            models: manifests
                .iter()
                .map(|manifest| {
                    status_for_manifest(
                        manifest,
                        TranslationModelPhase::Checking,
                        0,
                        Some("Checking native translation model files…".into()),
                    )
                })
                .collect(),
        })
        .unwrap_or_else(|error| inspection_failure(error.to_string()));
    *runtime.catalog.lock() = checking;
    runtime.inspecting.store(true, Ordering::Release);

    let app_for_worker = app.clone();
    let catalog = Arc::clone(&runtime.catalog);
    let inspecting = Arc::clone(&runtime.inspecting);
    let spawn_result = thread::Builder::new()
        .name("translation-model-inspection".into())
        .spawn(move || {
            let started = Instant::now();
            let next = inspect(&app_for_worker).unwrap_or_else(inspection_failure);
            let installed_models = next
                .models
                .iter()
                .filter(|model| model.phase == TranslationModelPhase::Ready)
                .count();
            *catalog.lock() = next.clone();
            inspecting.store(false, Ordering::Release);
            tracing::info!(
                elapsed_ms = started.elapsed().as_millis(),
                installed_models,
                "translation model catalog inspection completed"
            );
            publish(&app_for_worker, next);
        });
    if let Err(error) = spawn_result {
        runtime.inspecting.store(false, Ordering::Release);
        let next = inspection_failure(format!(
            "Could not start translation model inspection: {error}"
        ));
        *runtime.catalog.lock() = next.clone();
        publish(app, next);
    }
}

#[tauri::command]
pub fn translation_model_status(state: State<'_, RuntimeState>) -> TranslationStorageCatalog {
    state.translation.catalog.lock().clone()
}

#[tauri::command]
pub fn install_translation_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    storage_id: String,
) -> Result<(), String> {
    let manifest = translation_manifest(&storage_id).map_err(|error| error.to_string())?;
    require_catalog_ready(&state)?;
    require_stopped(&state)?;
    let root = models_root(&app)?;
    state
        .translation
        .installing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "Another translation model is already downloading.".to_owned())?;

    update_model(
        &app,
        &state.translation.catalog,
        &manifest.id,
        TranslationModelPhase::Downloading,
        0,
        Some(format!(
            "Downloading and verifying {}…",
            manifest.display_name
        )),
    );

    let catalog = Arc::clone(&state.translation.catalog);
    let installing = Arc::clone(&state.translation.installing);
    let app_for_worker = app.clone();
    let spawn_result = thread::Builder::new()
        .name("translation-model-download".into())
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
                    TranslationModelPhase::Ready,
                    manifest.download_size_bytes(),
                    None,
                ),
                Err(error) => {
                    tracing::error!(
                        storage_id = %manifest.id,
                        %error,
                        "translation model installation failed"
                    );
                    let phase = if matches!(error, ModelManagerError::Integrity(_, _)) {
                        TranslationModelPhase::Corrupt
                    } else {
                        TranslationModelPhase::Failed
                    };
                    update_model(
                        &app_for_worker,
                        &catalog,
                        &manifest.id,
                        phase,
                        0,
                        Some(error.to_string()),
                    );
                }
            }
            installing.store(false, Ordering::Release);
        });
    if let Err(error) = spawn_result {
        state.translation.installing.store(false, Ordering::Release);
        let message = format!("Could not start the translation model download: {error}");
        update_model(
            &app,
            &state.translation.catalog,
            &storage_id,
            TranslationModelPhase::Failed,
            0,
            Some(message.clone()),
        );
        return Err(message);
    }
    Ok(())
}

#[tauri::command]
pub fn remove_translation_model(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    storage_id: String,
) -> Result<(), String> {
    let manifest = translation_manifest(&storage_id).map_err(|error| error.to_string())?;
    let _control = state.control.lock();
    require_catalog_ready(&state)?;
    require_stopped(&state)?;
    if state.translation.installing.load(Ordering::Acquire) {
        return Err(
            "Wait for the current translation model download to finish before removing a model."
                .into(),
        );
    }
    ModelManager::new(models_root(&app)?)
        .remove(&manifest)
        .map_err(|error| error.to_string())?;
    update_model(
        &app,
        &state.translation.catalog,
        &manifest.id,
        TranslationModelPhase::NotInstalled,
        0,
        None,
    );
    Ok(())
}

pub fn model_protocol_response(
    app: &AppHandle,
    webview_label: &str,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    if webview_label != "main" {
        return protocol_error(
            StatusCode::FORBIDDEN,
            "Model files are private to the main UI.",
        );
    }
    if request.method() == tauri::http::Method::OPTIONS {
        return protocol_response(StatusCode::NO_CONTENT, Vec::new());
    }
    if request.method() != tauri::http::Method::GET {
        return protocol_error(
            StatusCode::METHOD_NOT_ALLOWED,
            "Only model reads are allowed.",
        );
    }

    match read_protocol_resource(app, &request) {
        Ok(response) => response,
        Err((status, message)) => protocol_error(status, &message),
    }
}

fn read_protocol_resource(
    app: &AppHandle,
    request: &Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, (StatusCode, String)> {
    let mut segments = request.uri().path().trim_start_matches('/').split('/');
    if segments.next() != Some(MODEL_PROTOCOL_PREFIX) {
        return Err((StatusCode::NOT_FOUND, "Unknown model resource.".into()));
    }
    let storage_id = segments
        .next()
        .ok_or((StatusCode::NOT_FOUND, "Missing model identifier.".into()))?;
    let resource = segments.collect::<Vec<_>>().join("/");
    if resource.is_empty() {
        return Err((StatusCode::NOT_FOUND, "Missing model resource.".into()));
    }

    let state = app.state::<RuntimeState>();
    let ready =
        state.translation.catalog.lock().models.iter().any(|model| {
            model.storage_id == storage_id && model.phase == TranslationModelPhase::Ready
        });
    if !ready {
        return Err((
            StatusCode::NOT_FOUND,
            "The native model is not ready.".into(),
        ));
    }

    let manifest = translation_manifest(storage_id)
        .map_err(|error| (StatusCode::NOT_FOUND, error.to_string()))?;
    let manager = ModelManager::new(
        models_root(app).map_err(|message| (StatusCode::INTERNAL_SERVER_ERROR, message))?,
    );

    if resource == VERIFIED_RESOURCE {
        if !matches!(manager.state(&manifest), Ok(ModelInstallState::Ready)) {
            return Err((
                StatusCode::CONFLICT,
                "The native model no longer passes verification.".into(),
            ));
        }
        return Ok(protocol_response(
            StatusCode::OK,
            br#"{"storage":"native"}"#.to_vec(),
        ));
    }

    let artifact = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.path == resource)
        .ok_or((StatusCode::NOT_FOUND, "Unknown model artifact.".into()))?;
    let directory = manager
        .model_directory(&manifest)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Native model artifacts require an explicit byte range.".into(),
        ))?;
    let (start, end) = parse_range(range, artifact.size_bytes)?;
    let length = end - start + 1;
    let path = directory.join(&artifact.path);
    let mut file = File::open(&path).map_err(|error| {
        (
            StatusCode::NOT_FOUND,
            format!("Could not open the verified model artifact: {error}"),
        )
    })?;
    let actual_bytes = file
        .metadata()
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Could not inspect the verified model artifact: {error}"),
            )
        })?
        .len();
    if actual_bytes != artifact.size_bytes {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "The native model artifact changed size ({actual_bytes} instead of {}).",
                artifact.size_bytes
            ),
        ));
    }
    file.seek(SeekFrom::Start(start)).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Could not seek in the model artifact: {error}"),
        )
    })?;
    let mut bytes = vec![0_u8; length as usize];
    file.read_exact(&mut bytes).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Could not read the verified model artifact: {error}"),
        )
    })?;

    let content_type = if artifact.path.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    };
    Ok(Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_EXPOSE_HEADERS, "content-range")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, length)
        .header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{}", artifact.size_bytes),
        )
        .body(bytes)
        .expect("static model protocol headers are valid"))
}

fn parse_range(value: &str, total_bytes: u64) -> Result<(u64, u64), (StatusCode, String)> {
    let raw = value.strip_prefix("bytes=").ok_or((
        StatusCode::RANGE_NOT_SATISFIABLE,
        "Only byte ranges are supported.".into(),
    ))?;
    if raw.contains(',') {
        return Err((
            StatusCode::RANGE_NOT_SATISFIABLE,
            "Multipart model ranges are not supported.".into(),
        ));
    }
    let (start, requested_end) = raw.split_once('-').ok_or((
        StatusCode::RANGE_NOT_SATISFIABLE,
        "The model byte range is invalid.".into(),
    ))?;
    let start = start.parse::<u64>().map_err(|_| {
        (
            StatusCode::RANGE_NOT_SATISFIABLE,
            "The model byte range start is invalid.".into(),
        )
    })?;
    if start >= total_bytes {
        return Err((
            StatusCode::RANGE_NOT_SATISFIABLE,
            "The model byte range begins after the artifact.".into(),
        ));
    }
    let requested_end = if requested_end.is_empty() {
        total_bytes - 1
    } else {
        requested_end.parse::<u64>().map_err(|_| {
            (
                StatusCode::RANGE_NOT_SATISFIABLE,
                "The model byte range end is invalid.".into(),
            )
        })?
    };
    if requested_end < start {
        return Err((
            StatusCode::RANGE_NOT_SATISFIABLE,
            "The model byte range is reversed.".into(),
        ));
    }
    let bounded_end = requested_end
        .min(total_bytes - 1)
        .min(start.saturating_add(MAX_PROTOCOL_CHUNK_BYTES - 1));
    Ok((start, bounded_end))
}

fn protocol_response(status: StatusCode, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS")
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
        .header(header::CONTENT_LENGTH, body.len())
        .body(body)
        .expect("static model protocol headers are valid")
}

fn protocol_error(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .expect("static model protocol headers are valid")
}

fn require_stopped(state: &RuntimeState) -> Result<(), String> {
    if state.supervisor.lock().has_active_session() {
        Err("Stop captions or screen translation before changing translation models.".into())
    } else {
        Ok(())
    }
}

fn require_catalog_ready(state: &RuntimeState) -> Result<(), String> {
    if state.translation.inspecting.load(Ordering::Acquire) {
        Err("Wait for Prollyglot to finish checking native translation models.".into())
    } else {
        Ok(())
    }
}

fn inspect(app: &AppHandle) -> Result<TranslationStorageCatalog, String> {
    let manifests = translation_model_manifests().map_err(|error| error.to_string())?;
    let manager = ModelManager::new(models_root(app)?);
    let models = manifests
        .iter()
        .map(|manifest| match manager.state(manifest) {
            Ok(ModelInstallState::NotInstalled) => {
                status_for_manifest(manifest, TranslationModelPhase::NotInstalled, 0, None)
            }
            Ok(ModelInstallState::Ready) => status_for_manifest(
                manifest,
                TranslationModelPhase::Ready,
                manifest.download_size_bytes(),
                None,
            ),
            Ok(ModelInstallState::Corrupt { issues }) => status_for_manifest(
                manifest,
                TranslationModelPhase::Corrupt,
                0,
                Some(format!(
                    "The native model is incomplete or corrupt: {}",
                    issues.join("; ")
                )),
            ),
            Err(error) => status_for_manifest(
                manifest,
                TranslationModelPhase::Failed,
                0,
                Some(error.to_string()),
            ),
        })
        .collect();
    Ok(TranslationStorageCatalog { models })
}

fn inspection_failure(message: String) -> TranslationStorageCatalog {
    match translation_model_manifests() {
        Ok(manifests) => TranslationStorageCatalog {
            models: manifests
                .iter()
                .map(|manifest| {
                    status_for_manifest(
                        manifest,
                        TranslationModelPhase::Failed,
                        0,
                        Some(message.clone()),
                    )
                })
                .collect(),
        },
        Err(_) => TranslationStorageCatalog::default(),
    }
}

fn status_for_manifest(
    manifest: &ModelManifest,
    phase: TranslationModelPhase,
    downloaded_bytes: u64,
    message: Option<String>,
) -> TranslationStorageStatus {
    TranslationStorageStatus {
        phase,
        storage_id: manifest.id.clone(),
        downloaded_bytes,
        total_bytes: manifest.download_size_bytes(),
        message,
    }
}

fn publish_progress(
    app: &AppHandle,
    catalog: &Arc<Mutex<TranslationStorageCatalog>>,
    progress: DownloadProgress,
) {
    update_model(
        app,
        catalog,
        &progress.model_id,
        TranslationModelPhase::Downloading,
        progress.completed_bytes,
        Some(format!("Downloading {}…", progress.artifact_role)),
    );
}

fn update_model(
    app: &AppHandle,
    catalog: &Arc<Mutex<TranslationStorageCatalog>>,
    storage_id: &str,
    phase: TranslationModelPhase,
    downloaded_bytes: u64,
    message: Option<String>,
) {
    let next = {
        let mut catalog = catalog.lock();
        let Some(model) = catalog
            .models
            .iter_mut()
            .find(|model| model.storage_id == storage_id)
        else {
            tracing::error!(%storage_id, "could not update an unknown translation model");
            return;
        };
        model.phase = phase;
        model.downloaded_bytes = downloaded_bytes.min(model.total_bytes);
        model.message = message;
        catalog.clone()
    };
    publish(app, next);
}

fn publish(app: &AppHandle, next: TranslationStorageCatalog) {
    if let Err(error) = app.emit(TRANSLATION_MODEL_STATUS_EVENT, next) {
        tracing::warn!(%error, "could not emit translation model status");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_ranges_are_bounded_and_reject_invalid_requests() {
        assert_eq!(
            parse_range("bytes=0-99", 1_000).expect("small range"),
            (0, 99)
        );
        assert_eq!(
            parse_range("bytes=0-99999999", 10_000_000).expect("bounded range"),
            (0, MAX_PROTOCOL_CHUNK_BYTES - 1)
        );
        assert!(parse_range("bytes=100-50", 1_000).is_err());
        assert!(parse_range("bytes=1000-", 1_000).is_err());
        assert!(parse_range("items=0-10", 1_000).is_err());
    }

    #[test]
    fn protocol_paths_can_only_select_manifest_artifacts() {
        let manifest = translation_manifest("translation-opus-mt-ja-en").expect("manifest");
        assert!(
            manifest
                .artifacts
                .iter()
                .any(|artifact| artifact.path == "onnx/encoder_model_quantized.onnx")
        );
        assert!(
            !manifest
                .artifacts
                .iter()
                .any(|artifact| artifact.path == "../outside")
        );
    }
}
