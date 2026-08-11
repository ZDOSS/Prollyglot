//! Explicit, integrity-checked management of separately downloaded ASR models.

use std::{
    collections::{BTreeMap, HashSet},
    ffi::OsString,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use prollyglot_asr::ModelLocation;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MANIFEST_SCHEMA_VERSION: u32 = 1;
const DOWNLOAD_BUFFER_SIZE: usize = 64 * 1024;
const USER_AGENT: &str = concat!("Prollyglot/", env!("CARGO_PKG_VERSION"));
const VERIFICATION_MARKER_FILE: &str = ".prollyglot-verified.json";
const VERIFICATION_MARKER_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_ENGLISH_MODEL_ID: &str = "sherpa-zipformer-en-20m-2023-02-17";
pub const DEFAULT_SPEECH_MODEL_ID: &str = DEFAULT_ENGLISH_MODEL_ID;
pub const NEMOTRON_MULTILINGUAL_MODEL_ID: &str =
    "nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProvenance {
    pub name: String,
    pub version: String,
    pub license: String,
    pub source_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProvenance {
    pub source_url: String,
    pub revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelArtifact {
    pub role: String,
    pub path: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub display_name: String,
    pub backend: String,
    pub languages: Vec<String>,
    pub license: String,
    pub provenance: ModelProvenance,
    pub runtime: RuntimeProvenance,
    pub approximate_memory_bytes: Option<u64>,
    pub artifacts: Vec<ModelArtifact>,
}

impl ModelManifest {
    pub fn from_json(json: &str) -> Result<Self, ModelManagerError> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|error| ModelManagerError::InvalidManifest(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ModelManagerError> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(ModelManagerError::InvalidManifest(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        validate_identifier("model id", &self.id)?;
        validate_identifier("model version", &self.version)?;
        if self.display_name.trim().is_empty()
            || self.backend.trim().is_empty()
            || self.license.trim().is_empty()
            || self.languages.is_empty()
            || self.artifacts.is_empty()
        {
            return Err(ModelManagerError::InvalidManifest(
                "name, backend, license, languages, and artifacts are required".into(),
            ));
        }
        if self
            .languages
            .iter()
            .any(|language| language.trim().is_empty())
        {
            return Err(ModelManagerError::InvalidManifest(
                "language tags must not be empty".into(),
            ));
        }
        validate_https_url("model provenance", &self.provenance.source_url)?;
        validate_https_url("runtime provenance", &self.runtime.source_url)?;
        if self.provenance.revision.trim().is_empty()
            || self.runtime.name.trim().is_empty()
            || self.runtime.version.trim().is_empty()
            || self.runtime.license.trim().is_empty()
        {
            return Err(ModelManagerError::InvalidManifest(
                "model revision and runtime provenance are required".into(),
            ));
        }

        let mut roles = HashSet::new();
        let mut paths = HashSet::new();
        for artifact in &self.artifacts {
            if artifact.role.trim().is_empty() || !roles.insert(artifact.role.as_str()) {
                return Err(ModelManagerError::InvalidManifest(format!(
                    "artifact role {:?} is empty or duplicated",
                    artifact.role
                )));
            }
            validate_relative_path(&artifact.path)?;
            if !paths.insert(artifact.path.as_str()) {
                return Err(ModelManagerError::InvalidManifest(format!(
                    "artifact path {:?} is duplicated",
                    artifact.path
                )));
            }
            validate_https_url("artifact", &artifact.url)?;
            if artifact.size_bytes == 0 {
                return Err(ModelManagerError::InvalidManifest(format!(
                    "artifact {:?} has an empty expected size",
                    artifact.path
                )));
            }
            if artifact.sha256.len() != 64
                || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(ModelManagerError::InvalidManifest(format!(
                    "artifact {:?} has an invalid SHA-256 digest",
                    artifact.path
                )));
            }
        }
        Ok(())
    }

    pub fn download_size_bytes(&self) -> u64 {
        self.artifacts
            .iter()
            .map(|artifact| artifact.size_bytes)
            .sum()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ModelInstallState {
    NotInstalled,
    Ready,
    Corrupt { issues: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub model_id: String,
    pub artifact_role: String,
    pub downloaded_bytes: u64,
    pub artifact_bytes: u64,
    pub completed_bytes: u64,
    pub model_bytes: u64,
}

#[derive(Debug, Error)]
pub enum ModelManagerError {
    #[error("invalid model manifest: {0}")]
    InvalidManifest(String),
    #[error("model {0} is not completely installed")]
    NotInstalled(String),
    #[error("model {0} failed integrity verification: {1}")]
    Integrity(String, String),
    #[error("could not download {url}: {message}")]
    Download { url: String, message: String },
    #[error("model file operation failed for {path}: {message}")]
    File { path: PathBuf, message: String },
}

pub struct ModelManager {
    root: PathBuf,
    http: ureq::Agent,
}

#[derive(Debug, Deserialize, Serialize)]
struct VerificationMarker {
    schema_version: u32,
    manifest_digest: String,
    artifacts: Vec<VerifiedArtifactMetadata>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
struct VerifiedArtifactMetadata {
    path: String,
    size_bytes: u64,
    modified_seconds: u64,
    modified_nanoseconds: u32,
}

impl ModelManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            http: http_agent(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn model_directory(&self, manifest: &ModelManifest) -> Result<PathBuf, ModelManagerError> {
        manifest.validate()?;
        Ok(self.root.join(&manifest.id).join(&manifest.version))
    }

    pub fn state(&self, manifest: &ModelManifest) -> Result<ModelInstallState, ModelManagerError> {
        let directory = self.model_directory(manifest)?;
        let mut present_artifacts = 0;
        let mut issues = Vec::new();
        for artifact in &manifest.artifacts {
            let path = directory.join(&artifact.path);
            match fs::metadata(&path) {
                Ok(metadata) => {
                    present_artifacts += 1;
                    if !metadata.is_file() {
                        issues.push(format!("{} is not a regular file", artifact.path));
                    } else if metadata.len() != artifact.size_bytes {
                        issues.push(format!(
                            "{} has {} bytes; expected {}",
                            artifact.path,
                            metadata.len(),
                            artifact.size_bytes
                        ));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    issues.push(format!("{} is missing", artifact.path));
                }
                Err(error) => return Err(file_error(&path, error)),
            }
        }
        if present_artifacts == 0 {
            return Ok(ModelInstallState::NotInstalled);
        }
        if !issues.is_empty() {
            remove_verification_marker(&directory);
            return Ok(ModelInstallState::Corrupt { issues });
        }
        if verification_marker_matches(&directory, manifest)? {
            return Ok(ModelInstallState::Ready);
        }

        for artifact in &manifest.artifacts {
            let path = directory.join(&artifact.path);
            match verify_artifact(&path, artifact)? {
                ArtifactState::Ready => {}
                ArtifactState::Missing => issues.push(format!("{} is missing", artifact.path)),
                ArtifactState::Invalid(issue) => issues.push(issue),
            }
        }
        if !issues.is_empty() {
            remove_verification_marker(&directory);
            return Ok(ModelInstallState::Corrupt { issues });
        }
        // A marker is only a launch optimization. Failure to write it must not
        // make an already verified local model unusable.
        let _ = write_verification_marker(&directory, manifest);
        Ok(ModelInstallState::Ready)
    }

    /// Download a model only when explicitly called. Each artifact is written
    /// to a sidecar file, verified, and then atomically renamed. A process or
    /// network interruption can therefore never make a partial file look ready;
    /// the next call restarts only unfinished or corrupt artifacts.
    pub fn install<F>(
        &self,
        manifest: &ModelManifest,
        mut progress: F,
    ) -> Result<ModelLocation, ModelManagerError>
    where
        F: FnMut(DownloadProgress),
    {
        let directory = self.model_directory(manifest)?;
        fs::create_dir_all(&directory).map_err(|error| file_error(&directory, error))?;
        remove_verification_marker(&directory);
        let mut completed_bytes = 0_u64;
        for artifact in &manifest.artifacts {
            let target = directory.join(&artifact.path);
            if matches!(verify_artifact(&target, artifact)?, ArtifactState::Ready) {
                completed_bytes = completed_bytes.saturating_add(artifact.size_bytes);
                progress(DownloadProgress {
                    model_id: manifest.id.clone(),
                    artifact_role: artifact.role.clone(),
                    downloaded_bytes: artifact.size_bytes,
                    artifact_bytes: artifact.size_bytes,
                    completed_bytes,
                    model_bytes: manifest.download_size_bytes(),
                });
                continue;
            }

            let parent = target.parent().ok_or_else(|| {
                ModelManagerError::InvalidManifest(format!(
                    "artifact {:?} has no parent directory",
                    artifact.path
                ))
            })?;
            fs::create_dir_all(parent).map_err(|error| file_error(parent, error))?;
            let mut response = self
                .http
                .get(&artifact.url)
                .header("User-Agent", USER_AGENT)
                .call()
                .map_err(|error| ModelManagerError::Download {
                    url: artifact.url.clone(),
                    message: error.to_string(),
                })?;
            let reader = response.body_mut().as_reader();
            install_artifact(
                &target,
                artifact,
                reader,
                completed_bytes,
                manifest.download_size_bytes(),
                &manifest.id,
                &mut progress,
            )?;
            completed_bytes = completed_bytes.saturating_add(artifact.size_bytes);
        }
        let _ = write_verification_marker(&directory, manifest);
        self.location(manifest)
    }

    pub fn location(&self, manifest: &ModelManifest) -> Result<ModelLocation, ModelManagerError> {
        let directory = self.model_directory(manifest)?;
        match self.state(manifest)? {
            ModelInstallState::Ready => {}
            ModelInstallState::NotInstalled => {
                return Err(ModelManagerError::NotInstalled(manifest.id.clone()));
            }
            ModelInstallState::Corrupt { issues } => {
                return Err(ModelManagerError::Integrity(
                    manifest.id.clone(),
                    issues.join("; "),
                ));
            }
        }
        let artifacts = manifest
            .artifacts
            .iter()
            .map(|artifact| (artifact.role.clone(), directory.join(&artifact.path)))
            .collect::<BTreeMap<_, _>>();
        Ok(ModelLocation {
            id: manifest.id.clone(),
            backend: manifest.backend.clone(),
            languages: manifest.languages.clone(),
            directory,
            artifacts,
        })
    }

    pub fn remove(&self, manifest: &ModelManifest) -> Result<(), ModelManagerError> {
        let directory = self.model_directory(manifest)?;
        if directory.exists() {
            fs::remove_dir_all(&directory).map_err(|error| file_error(&directory, error))?;
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn http_agent() -> ureq::Agent {
    ureq::config::Config::builder()
        .https_only(true)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(Duration::from_secs(60 * 60)))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent()
}

#[cfg(not(target_os = "windows"))]
fn http_agent() -> ureq::Agent {
    ureq::config::Config::builder()
        .https_only(true)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(Duration::from_secs(60 * 60)))
        .build()
        .new_agent()
}

pub fn initial_english_manifest() -> Result<ModelManifest, ModelManagerError> {
    ModelManifest::from_json(include_str!(
        "../../../assets/model-manifests/english-streaming-small.json"
    ))
}

/// Standard-size English streaming model.
pub fn comparison_english_manifest() -> Result<ModelManifest, ModelManagerError> {
    ModelManifest::from_json(include_str!(
        "../../../assets/model-manifests/english-streaming-standard.json"
    ))
}

/// Larger English streaming model trained on LibriSpeech and GigaSpeech.
pub fn enhanced_english_manifest() -> Result<ModelManifest, ModelManagerError> {
    ModelManifest::from_json(include_str!(
        "../../../assets/model-manifests/english-streaming-enhanced.json"
    ))
}

/// Compact streaming French model trained on Common Voice.
pub fn compact_french_manifest() -> Result<ModelManifest, ModelManagerError> {
    ModelManifest::from_json(include_str!(
        "../../../assets/model-manifests/french-streaming-compact.json"
    ))
}

/// Compact streaming Korean model trained on KsponSpeech.
pub fn compact_korean_manifest() -> Result<ModelManifest, ModelManagerError> {
    ModelManifest::from_json(include_str!(
        "../../../assets/model-manifests/korean-streaming-compact.json"
    ))
}

/// Small 14M streaming Mandarin model trained on WenetSpeech.
pub fn small_chinese_manifest() -> Result<ModelManifest, ModelManagerError> {
    ModelManifest::from_json(include_str!(
        "../../../assets/model-manifests/chinese-streaming-small.json"
    ))
}

/// Compact streaming Bengali model converted from the Vosk release.
pub fn compact_bengali_manifest() -> Result<ModelManifest, ModelManagerError> {
    ModelManifest::from_json(include_str!(
        "../../../assets/model-manifests/bengali-streaming-compact.json"
    ))
}

/// NVIDIA's multilingual 0.6B streaming model converted to pinned INT8 ONNX
/// artifacts for sherpa-onnx. The 560 ms checkpoint is the balanced streaming
/// profile; larger upstream chunks trade additional delay for accuracy.
pub fn nemotron_multilingual_manifest() -> Result<ModelManifest, ModelManagerError> {
    ModelManifest::from_json(include_str!(
        "../../../assets/model-manifests/nemotron-3.5-streaming-multilingual.json"
    ))
}

/// Built-in English choices in the order presented by the product.
pub fn english_model_manifests() -> Result<Vec<ModelManifest>, ModelManagerError> {
    Ok(vec![
        initial_english_manifest()?,
        comparison_english_manifest()?,
        enhanced_english_manifest()?,
    ])
}

pub fn english_manifest(model_id: &str) -> Result<ModelManifest, ModelManagerError> {
    english_model_manifests()?
        .into_iter()
        .find(|manifest| manifest.id == model_id)
        .ok_or_else(|| {
            ModelManagerError::InvalidManifest(format!(
                "unknown built-in English model {model_id:?}"
            ))
        })
}

/// Smaller language-specific streaming choices that avoid loading the 0.6B
/// multilingual model when one of these languages is all the user needs.
pub fn compact_language_manifests() -> Result<Vec<ModelManifest>, ModelManagerError> {
    Ok(vec![
        small_chinese_manifest()?,
        compact_french_manifest()?,
        compact_korean_manifest()?,
        compact_bengali_manifest()?,
    ])
}

/// Every speech model currently exposed by Prollyglot, in product order.
pub fn speech_model_manifests() -> Result<Vec<ModelManifest>, ModelManagerError> {
    let mut manifests = english_model_manifests()?;
    manifests.extend(compact_language_manifests()?);
    manifests.push(nemotron_multilingual_manifest()?);
    Ok(manifests)
}

pub fn speech_manifest(model_id: &str) -> Result<ModelManifest, ModelManagerError> {
    speech_model_manifests()?
        .into_iter()
        .find(|manifest| manifest.id == model_id)
        .ok_or_else(|| {
            ModelManagerError::InvalidManifest(format!(
                "unknown built-in speech model {model_id:?}"
            ))
        })
}

fn manifest_digest(manifest: &ModelManifest) -> Result<String, ModelManagerError> {
    let serialized = serde_json::to_vec(manifest).map_err(|error| {
        ModelManagerError::InvalidManifest(format!(
            "could not serialize the model manifest for verification: {error}"
        ))
    })?;
    let mut digest = Sha256::new();
    digest.update(serialized);
    Ok(hex_lower(&digest.finalize()))
}

fn artifact_metadata(
    directory: &Path,
    artifact: &ModelArtifact,
) -> Result<Option<VerifiedArtifactMetadata>, ModelManagerError> {
    let path = directory.join(&artifact.path);
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(file_error(&path, error)),
    };
    if !metadata.is_file() || metadata.len() != artifact.size_bytes {
        return Ok(None);
    }
    let modified = metadata
        .modified()
        .map_err(|error| file_error(&path, error))?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    Ok(Some(VerifiedArtifactMetadata {
        path: artifact.path.clone(),
        size_bytes: metadata.len(),
        modified_seconds: modified.as_secs(),
        modified_nanoseconds: modified.subsec_nanos(),
    }))
}

fn verification_marker_matches(
    directory: &Path,
    manifest: &ModelManifest,
) -> Result<bool, ModelManagerError> {
    let path = directory.join(VERIFICATION_MARKER_FILE);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        // The marker is an optimization rather than model data. Fall back to
        // full verification if it cannot be read or parsed.
        Err(_) => return Ok(false),
    };
    let marker: VerificationMarker = match serde_json::from_slice(&bytes) {
        Ok(marker) => marker,
        Err(_) => return Ok(false),
    };
    if marker.schema_version != VERIFICATION_MARKER_SCHEMA_VERSION
        || marker.manifest_digest != manifest_digest(manifest)?
        || marker.artifacts.len() != manifest.artifacts.len()
    {
        return Ok(false);
    }
    for (artifact, expected) in manifest.artifacts.iter().zip(&marker.artifacts) {
        if artifact_metadata(directory, artifact)?.as_ref() != Some(expected) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn write_verification_marker(
    directory: &Path,
    manifest: &ModelManifest,
) -> Result<(), ModelManagerError> {
    let artifacts = manifest
        .artifacts
        .iter()
        .map(|artifact| {
            artifact_metadata(directory, artifact)?.ok_or_else(|| {
                ModelManagerError::Integrity(
                    manifest.id.clone(),
                    format!("{} changed before verification completed", artifact.path),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let marker = VerificationMarker {
        schema_version: VERIFICATION_MARKER_SCHEMA_VERSION,
        manifest_digest: manifest_digest(manifest)?,
        artifacts,
    };
    let bytes = serde_json::to_vec(&marker).map_err(|error| {
        ModelManagerError::InvalidManifest(format!(
            "could not serialize the model verification marker: {error}"
        ))
    })?;
    let path = directory.join(VERIFICATION_MARKER_FILE);
    fs::write(&path, bytes).map_err(|error| file_error(&path, error))
}

fn remove_verification_marker(directory: &Path) {
    let path = directory.join(VERIFICATION_MARKER_FILE);
    // A stale or unreadable marker never makes a model ready; a later state
    // check will continue to use full verification if removal fails.
    let _ = fs::remove_file(path);
}

enum ArtifactState {
    Missing,
    Ready,
    Invalid(String),
}

fn verify_artifact(
    path: &Path,
    artifact: &ModelArtifact,
) -> Result<ArtifactState, ModelManagerError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(ArtifactState::Missing),
        Err(error) => return Err(file_error(path, error)),
    };
    if !metadata.is_file() {
        return Ok(ArtifactState::Invalid(format!(
            "{} is not a regular file",
            artifact.path
        )));
    }
    if metadata.len() != artifact.size_bytes {
        return Ok(ArtifactState::Invalid(format!(
            "{} has {} bytes; expected {}",
            artifact.path,
            metadata.len(),
            artifact.size_bytes
        )));
    }
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(&artifact.sha256) {
        return Ok(ArtifactState::Invalid(format!(
            "{} has SHA-256 {}; expected {}",
            artifact.path, actual, artifact.sha256
        )));
    }
    Ok(ArtifactState::Ready)
}

#[allow(clippy::too_many_arguments)]
fn install_artifact<R, F>(
    target: &Path,
    artifact: &ModelArtifact,
    mut reader: R,
    completed_bytes: u64,
    model_bytes: u64,
    model_id: &str,
    progress: &mut F,
) -> Result<(), ModelManagerError>
where
    R: Read,
    F: FnMut(DownloadProgress),
{
    let partial = sidecar_path(target, ".partial")?;
    let mut output = File::create(&partial).map_err(|error| file_error(&partial, error))?;
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
    let mut downloaded = 0_u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| file_error(&partial, error))?;
        if read == 0 {
            break;
        }
        downloaded = downloaded.saturating_add(read as u64);
        if downloaded > artifact.size_bytes {
            return Err(ModelManagerError::Integrity(
                artifact.path.clone(),
                format!(
                    "download exceeded the expected {} bytes",
                    artifact.size_bytes
                ),
            ));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| file_error(&partial, error))?;
        progress(DownloadProgress {
            model_id: model_id.into(),
            artifact_role: artifact.role.clone(),
            downloaded_bytes: downloaded,
            artifact_bytes: artifact.size_bytes,
            completed_bytes: completed_bytes.saturating_add(downloaded),
            model_bytes,
        });
    }
    output
        .sync_all()
        .map_err(|error| file_error(&partial, error))?;
    drop(output);

    match verify_artifact(&partial, artifact)? {
        ArtifactState::Ready => {}
        ArtifactState::Missing => {
            return Err(ModelManagerError::Integrity(
                artifact.path.clone(),
                "downloaded sidecar disappeared before verification".into(),
            ));
        }
        ArtifactState::Invalid(issue) => {
            let _ = fs::remove_file(&partial);
            return Err(ModelManagerError::Integrity(artifact.path.clone(), issue));
        }
    }

    if target.exists() {
        fs::remove_file(target).map_err(|error| file_error(target, error))?;
    }
    fs::rename(&partial, target).map_err(|error| file_error(target, error))?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, ModelManagerError> {
    let mut file = File::open(path).map_err(|error| file_error(path, error))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| file_error(path, error))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex_lower(&digest.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn validate_identifier(label: &str, value: &str) -> Result<(), ModelManagerError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ModelManagerError::InvalidManifest(format!(
            "{label} must contain only ASCII letters, numbers, dots, dashes, or underscores"
        )));
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<(), ModelManagerError> {
    let path = Path::new(value);
    if value.is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ModelManagerError::InvalidManifest(format!(
            "artifact path {value:?} must be a safe relative path"
        )));
    }
    Ok(())
}

fn validate_https_url(label: &str, value: &str) -> Result<(), ModelManagerError> {
    if !value.starts_with("https://") {
        return Err(ModelManagerError::InvalidManifest(format!(
            "{label} URL must use HTTPS"
        )));
    }
    Ok(())
}

fn sidecar_path(target: &Path, suffix: &str) -> Result<PathBuf, ModelManagerError> {
    let file_name = target.file_name().ok_or_else(|| {
        ModelManagerError::InvalidManifest("artifact target has no file name".into())
    })?;
    let mut sidecar_name = OsString::from(file_name);
    sidecar_name.push(suffix);
    Ok(target.with_file_name(sidecar_name))
}

fn file_error(path: &Path, error: io::Error) -> ModelManagerError {
    ModelManagerError::File {
        path: path.to_owned(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn test_artifact(bytes: &[u8]) -> ModelArtifact {
        let mut digest = Sha256::new();
        digest.update(bytes);
        ModelArtifact {
            role: "test".into(),
            path: "model.bin".into(),
            url: "https://example.invalid/model.bin".into(),
            size_bytes: bytes.len() as u64,
            sha256: hex_lower(&digest.finalize()),
        }
    }

    fn test_manifest(bytes: &[u8]) -> ModelManifest {
        ModelManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            id: "test-model".into(),
            version: "1".into(),
            display_name: "Test model".into(),
            backend: "test-backend".into(),
            languages: vec!["en".into()],
            license: "Apache-2.0".into(),
            provenance: ModelProvenance {
                source_url: "https://example.invalid/model".into(),
                revision: "test-revision".into(),
            },
            runtime: RuntimeProvenance {
                name: "test-runtime".into(),
                version: "1".into(),
                license: "Apache-2.0".into(),
                source_url: "https://example.invalid/runtime".into(),
            },
            approximate_memory_bytes: None,
            artifacts: vec![test_artifact(bytes)],
        }
    }

    #[test]
    fn pinned_english_manifest_is_valid_and_small() {
        let manifest = initial_english_manifest().expect("built-in manifest");

        assert_eq!(manifest.backend, "sherpa-onnx-online-transducer");
        assert_eq!(manifest.languages, vec!["en"]);
        assert_eq!(manifest.license, "Apache-2.0");
        assert_eq!(manifest.download_size_bytes(), 45_202_074);
    }

    #[test]
    fn english_model_catalog_is_valid_and_distinct() {
        let lightweight = initial_english_manifest().expect("lightweight manifest");
        let standard = comparison_english_manifest().expect("comparison manifest");
        let enhanced = enhanced_english_manifest().expect("enhanced manifest");

        assert_eq!(standard.backend, "sherpa-onnx-online-transducer");
        assert_eq!(standard.languages, vec!["en"]);
        assert_eq!(standard.license, "Apache-2.0");
        assert_eq!(standard.download_size_bytes(), 73_440_167);
        assert_eq!(enhanced.backend, "sherpa-onnx-online-transducer");
        assert_eq!(enhanced.languages, vec!["en"]);
        assert_eq!(enhanced.license, "Apache-2.0");
        assert_eq!(enhanced.download_size_bytes(), 190_180_941);
        assert_ne!(standard.id, lightweight.id);
        assert_ne!(enhanced.id, lightweight.id);
        assert_ne!(enhanced.id, standard.id);
        assert_eq!(english_model_manifests().expect("English catalog").len(), 3);
        assert_eq!(
            english_manifest(DEFAULT_ENGLISH_MODEL_ID)
                .expect("default model")
                .id,
            lightweight.id
        );
        assert!(english_manifest("unknown").is_err());
    }

    #[test]
    fn multilingual_model_is_pinned_and_included_in_the_speech_catalog() {
        let manifest = nemotron_multilingual_manifest().expect("Nemotron manifest");

        assert_eq!(manifest.backend, "sherpa-onnx-online-nemotron");
        assert_eq!(
            manifest.languages,
            vec![
                "auto", "en", "ar", "bg", "zh", "hr", "cs", "da", "nl", "et", "fi", "fr", "de",
                "hi", "hu", "it", "ja", "ko", "nb", "pl", "pt", "ro", "ru", "sk", "es", "sv", "tr",
                "uk", "vi",
            ]
        );
        assert_eq!(manifest.license, "OpenMDW-1.1");
        assert_eq!(manifest.download_size_bytes(), 682_215_356);
        assert_eq!(manifest.id, NEMOTRON_MULTILINGUAL_MODEL_ID);
        assert_eq!(speech_model_manifests().expect("speech catalog").len(), 8);
        assert_eq!(
            speech_manifest(NEMOTRON_MULTILINGUAL_MODEL_ID)
                .expect("multilingual model")
                .id,
            manifest.id
        );
        assert!(speech_manifest("unknown").is_err());
    }

    #[test]
    fn compact_language_models_are_pinned_and_distinct() {
        let models = compact_language_manifests().expect("compact language models");
        let expected = [
            ("zh", 30_975_688),
            ("fr", 129_012_566),
            ("ko", 140_919_603),
            ("bn", 94_119_939),
        ];

        assert_eq!(models.len(), expected.len());
        for (model, (language, bytes)) in models.iter().zip(expected) {
            assert_eq!(model.backend, "sherpa-onnx-online-transducer");
            assert_eq!(model.languages, vec![language]);
            assert_eq!(model.license, "Apache-2.0");
            assert_eq!(model.download_size_bytes(), bytes);
        }
    }

    #[test]
    fn manifest_rejects_directory_traversal() {
        let mut manifest = initial_english_manifest().expect("built-in manifest");
        manifest.artifacts[0].path = "../outside.bin".into();

        assert!(matches!(
            manifest.validate(),
            Err(ModelManagerError::InvalidManifest(_))
        ));
    }

    #[test]
    fn artifact_becomes_visible_only_after_hash_verification() {
        let directory = tempfile::tempdir().expect("temporary model root");
        let target = directory.path().join("model.bin");
        let artifact = test_artifact(b"verified model bytes");
        let mut updates = Vec::new();

        install_artifact(
            &target,
            &artifact,
            Cursor::new(b"verified model bytes"),
            0,
            artifact.size_bytes,
            "test-model",
            &mut |progress| updates.push(progress),
        )
        .expect("install valid artifact");

        assert!(target.is_file());
        assert!(!sidecar_path(&target, ".partial").expect("sidecar").exists());
        assert!(matches!(
            verify_artifact(&target, &artifact).expect("verify"),
            ArtifactState::Ready
        ));
        assert_eq!(updates.last().expect("progress").downloaded_bytes, 20);
    }

    #[test]
    fn corrupt_download_never_replaces_an_existing_file() {
        let directory = tempfile::tempdir().expect("temporary model root");
        let target = directory.path().join("model.bin");
        fs::write(&target, b"existing").expect("seed target");
        let artifact = test_artifact(b"expected");

        let error = install_artifact(
            &target,
            &artifact,
            Cursor::new(b"tampered"),
            0,
            artifact.size_bytes,
            "test-model",
            &mut |_| {},
        )
        .expect_err("hash mismatch");

        assert!(matches!(error, ModelManagerError::Integrity(_, _)));
        assert_eq!(fs::read(&target).expect("existing target"), b"existing");
    }

    #[test]
    fn verified_marker_never_hides_changed_model_bytes() {
        let directory = tempfile::tempdir().expect("temporary model root");
        let manager = ModelManager::new(directory.path());
        let manifest = test_manifest(b"verified");
        let model_directory = manager.model_directory(&manifest).expect("model directory");
        fs::create_dir_all(&model_directory).expect("create model directory");
        fs::write(model_directory.join("model.bin"), b"verified").expect("seed model");

        assert_eq!(
            manager.state(&manifest).expect("first verification"),
            ModelInstallState::Ready
        );
        let marker_path = model_directory.join(VERIFICATION_MARKER_FILE);
        assert!(marker_path.is_file());

        // Force a metadata mismatch even on filesystems with coarse modified
        // timestamps, then replace the model with different same-size bytes.
        let mut marker: VerificationMarker =
            serde_json::from_slice(&fs::read(&marker_path).expect("read marker"))
                .expect("parse marker");
        marker.artifacts[0].modified_nanoseconds =
            marker.artifacts[0].modified_nanoseconds.wrapping_add(1);
        fs::write(
            &marker_path,
            serde_json::to_vec(&marker).expect("serialize stale marker"),
        )
        .expect("write stale marker");
        fs::write(model_directory.join("model.bin"), b"tampered").expect("tamper model");

        assert!(matches!(
            manager.state(&manifest).expect("detect tampering"),
            ModelInstallState::Corrupt { .. }
        ));
        assert!(!marker_path.exists());
    }

    #[test]
    #[ignore = "downloads and verifies the 45 MB initial English model"]
    fn installs_the_pinned_english_model_from_upstream() {
        let directory = tempfile::tempdir().expect("temporary model root");
        let manager = ModelManager::new(directory.path());
        let manifest = initial_english_manifest().expect("built-in manifest");

        let location = manager
            .install(&manifest, |_| {})
            .expect("download pinned model");

        assert_eq!(location.artifacts.len(), 4);
        assert_eq!(
            manager.state(&manifest).expect("inspect installed model"),
            ModelInstallState::Ready
        );
    }
}
