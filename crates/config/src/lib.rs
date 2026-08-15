use std::{
    cmp::Reverse,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

pub const CONFIGURATION_SCHEMA_VERSION: u16 = 1;
const CONFIGURATION_PREFIX: &str = "configuration-";
const CONFIGURATION_SUFFIX: &str = ".json";
const RETAINED_REVISIONS: usize = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum ViewMode {
    #[default]
    Full,
    Compact,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum CaptionOutputPreference {
    #[default]
    Original,
    Translated,
    Both,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum BilingualLayout {
    #[default]
    Stacked,
    SideBySide,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum OverlayPosition {
    TopCenter,
    #[default]
    BottomCenter,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct OverlaySettings {
    pub font_family: String,
    pub font_size: u16,
    pub text_color: String,
    pub translated_text_color: String,
    pub bilingual_layout: BilingualLayout,
    pub background_opacity: f32,
    pub width: u32,
    pub maximum_lines: u8,
    pub reading_time_seconds: u16,
    pub fade_duration_ms: u16,
    pub position: OverlayPosition,
    pub click_through: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(tag = "kind", rename_all = "camelCase")]
pub enum AudioSourcePreference {
    #[default]
    FollowSystemDefault,
    PlaybackDevice {
        #[serde(rename = "deviceId")]
        #[ts(rename = "deviceId")]
        device_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct CaptionPreferences {
    pub spoken_language: String,
    pub output_mode: CaptionOutputPreference,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub translation_target: Option<String>,
    pub audio_source: AudioSourcePreference,
}

impl Default for CaptionPreferences {
    fn default() -> Self {
        Self {
            spoken_language: "en".into(),
            output_mode: CaptionOutputPreference::Original,
            translation_target: Some("en".into()),
            audio_source: AudioSourcePreference::FollowSystemDefault,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum VisualSourcePreference {
    #[default]
    ApplicationWindow,
    Display,
    Region,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum VisualDetectionPreference {
    #[default]
    Focused,
    AllText,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct VisualPreferences {
    pub source_mode: VisualSourcePreference,
    pub source_language: String,
    pub target_language: String,
    pub detection_mode: VisualDetectionPreference,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub display_id: Option<String>,
}

impl Default for VisualPreferences {
    fn default() -> Self {
        Self {
            source_mode: VisualSourcePreference::ApplicationWindow,
            source_language: "ja".into(),
            target_language: "en".into(),
            detection_mode: VisualDetectionPreference::Focused,
            window_id: None,
            display_id: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ModelPreferences {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub speech_model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub visual_model_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ApplicationConfiguration {
    pub schema_version: u16,
    pub legacy_webview_imported: bool,
    pub view_mode: ViewMode,
    pub captions: CaptionPreferences,
    pub overlay: OverlaySettings,
    pub visual: VisualPreferences,
    pub models: ModelPreferences,
}

impl Default for ApplicationConfiguration {
    fn default() -> Self {
        Self {
            schema_version: CONFIGURATION_SCHEMA_VERSION,
            legacy_webview_imported: false,
            view_mode: ViewMode::Full,
            captions: CaptionPreferences::default(),
            overlay: OverlaySettings::default(),
            visual: VisualPreferences::default(),
            models: ModelPreferences::default(),
        }
    }
}

impl ApplicationConfiguration {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if self.schema_version != CONFIGURATION_SCHEMA_VERSION {
            return Err(ConfigurationError::Invalid(format!(
                "Configuration schema {} is not supported; expected {}.",
                self.schema_version, CONFIGURATION_SCHEMA_VERSION
            )));
        }
        validate_overlay(&self.overlay)?;
        validate_language(&self.captions.spoken_language, true)?;
        if let Some(target) = &self.captions.translation_target {
            validate_language(target, false)?;
        }
        if self.captions.output_mode != CaptionOutputPreference::Original
            && self.captions.translation_target.is_none()
        {
            return Err(ConfigurationError::Invalid(
                "Translated caption output requires a target language.".into(),
            ));
        }
        if let AudioSourcePreference::PlaybackDevice { device_id } = &self.captions.audio_source {
            validate_opaque_id("playback device", device_id)?;
        }
        validate_language(&self.visual.source_language, false)?;
        validate_language(&self.visual.target_language, false)?;
        if self.visual.source_language == self.visual.target_language {
            return Err(ConfigurationError::Invalid(
                "Visual source and target languages must differ.".into(),
            ));
        }
        if let Some(window_id) = &self.visual.window_id {
            validate_opaque_id("visual window", window_id)?;
        }
        if let Some(display_id) = &self.visual.display_id {
            validate_opaque_id("visual display", display_id)?;
        }
        if let Some(model_id) = &self.models.speech_model_id {
            validate_model_id(model_id)?;
        }
        if let Some(model_id) = &self.models.visual_model_id {
            validate_model_id(model_id)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ConfigurationSnapshot {
    #[ts(type = "number")]
    pub revision: u64,
    pub config: ApplicationConfiguration,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct UpdateConfigurationCommand {
    #[ts(type = "number")]
    pub expected_revision: u64,
    pub config: ApplicationConfiguration,
}

#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("{0}")]
    Invalid(String),
    #[error("Could not access the local configuration: {0}")]
    Io(#[from] io::Error),
    #[error("Could not decode the local configuration: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Configuration revision {actual} is stale; current revision is {current}.")]
    StaleRevision { actual: u64, current: u64 },
    #[error("The configuration revision limit has been reached.")]
    RevisionExhausted,
}

pub struct ConfigurationLoad {
    pub snapshot: ConfigurationSnapshot,
    pub recovered: bool,
}

pub struct ConfigurationRepository {
    root: PathBuf,
}

impl ConfigurationRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn load(&self) -> Result<ConfigurationLoad, ConfigurationError> {
        fs::create_dir_all(&self.root)?;
        self.remove_abandoned_temps()?;
        let mut diagnostics = Vec::new();
        let candidates = self.candidates()?;
        for (revision, path) in &candidates {
            match self.read_candidate(*revision, path) {
                Ok(Candidate::Current(snapshot)) => {
                    let diagnostic = (!diagnostics.is_empty()).then(|| diagnostics.join(" "));
                    return Ok(ConfigurationLoad {
                        snapshot: ConfigurationSnapshot {
                            diagnostic,
                            ..snapshot
                        },
                        recovered: !diagnostics.is_empty(),
                    });
                }
                Ok(Candidate::Migrated(config)) => {
                    let snapshot = self.publish(config, *revision)?;
                    diagnostics.push(format!(
                        "Migrated local configuration to schema {CONFIGURATION_SCHEMA_VERSION}."
                    ));
                    return Ok(ConfigurationLoad {
                        snapshot: ConfigurationSnapshot {
                            diagnostic: Some(diagnostics.join(" ")),
                            ..snapshot
                        },
                        recovered: true,
                    });
                }
                Err(error) => {
                    diagnostics.push(error.to_string());
                    self.quarantine(path)?;
                }
            }
        }

        let snapshot = self.publish(ApplicationConfiguration::default(), 0)?;
        let recovered = !diagnostics.is_empty();
        Ok(ConfigurationLoad {
            snapshot: ConfigurationSnapshot {
                diagnostic: recovered.then(|| {
                    format!(
                        "{} Restored safe configuration defaults.",
                        diagnostics.join(" ")
                    )
                }),
                ..snapshot
            },
            recovered,
        })
    }

    pub fn save(
        &self,
        current_revision: u64,
        expected_revision: u64,
        config: ApplicationConfiguration,
    ) -> Result<ConfigurationSnapshot, ConfigurationError> {
        if current_revision != expected_revision {
            return Err(ConfigurationError::StaleRevision {
                actual: expected_revision,
                current: current_revision,
            });
        }
        self.publish(config, current_revision)
    }

    fn publish(
        &self,
        config: ApplicationConfiguration,
        current_revision: u64,
    ) -> Result<ConfigurationSnapshot, ConfigurationError> {
        config.validate()?;
        let revision = current_revision
            .checked_add(1)
            .ok_or(ConfigurationError::RevisionExhausted)?;
        fs::create_dir_all(&self.root)?;
        let snapshot = ConfigurationSnapshot {
            revision,
            config,
            diagnostic: None,
        };
        let bytes = serde_json::to_vec_pretty(&snapshot)?;
        let temporary = self
            .root
            .join(format!(".{CONFIGURATION_PREFIX}{revision:020}.tmp"));
        let destination = self.root.join(configuration_filename(revision));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        if let Err(error) = (|| -> io::Result<()> {
            file.write_all(&bytes)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &destination)?;
            // The revision file is already complete and atomically visible.
            // A directory-sync failure may mean the newest revision is lost in
            // a power failure, but the retained prior revision still recovers;
            // it must not make this process believe a committed write failed.
            let _ = sync_directory(&self.root);
            Ok(())
        })() {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        // Retention is maintenance after the durable commit. Leaving an extra
        // fallback is safer than reporting a failed write after publication.
        let _ = self.remove_old_revisions();
        Ok(snapshot)
    }

    fn read_candidate(
        &self,
        filename_revision: u64,
        path: &Path,
    ) -> Result<Candidate, ConfigurationError> {
        let bytes = fs::read(path)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        let schema = value
            .get("config")
            .and_then(|config| config.get("schemaVersion"))
            .or_else(|| value.get("schemaVersion"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if schema > u64::from(CONFIGURATION_SCHEMA_VERSION) {
            return Err(ConfigurationError::Invalid(format!(
                "A newer local configuration schema ({schema}) was set aside."
            )));
        }
        if schema == u64::from(CONFIGURATION_SCHEMA_VERSION) && value.get("config").is_some() {
            let mut snapshot: ConfigurationSnapshot = serde_json::from_value(value)?;
            if snapshot.revision != filename_revision {
                return Err(ConfigurationError::Invalid(
                    "A configuration revision did not match its file identity.".into(),
                ));
            }
            snapshot.config.validate()?;
            snapshot.diagnostic = None;
            return Ok(Candidate::Current(snapshot));
        }
        let legacy: LegacyConfigurationV0 = serde_json::from_value(value)?;
        Ok(Candidate::Migrated(legacy.migrate()))
    }

    fn candidates(&self) -> Result<Vec<(u64, PathBuf)>, ConfigurationError> {
        let mut candidates = fs::read_dir(&self.root)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                parse_configuration_revision(&entry.file_name().to_string_lossy())
                    .map(|revision| (revision, entry.path()))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable_by_key(|candidate| Reverse(candidate.0));
        Ok(candidates)
    }

    fn remove_old_revisions(&self) -> Result<(), ConfigurationError> {
        for (_, path) in self.candidates()?.into_iter().skip(RETAINED_REVISIONS) {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn remove_abandoned_temps(&self) -> Result<(), ConfigurationError> {
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&format!(".{CONFIGURATION_PREFIX}")) && name.ends_with(".tmp") {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }

    fn quarantine(&self, path: &Path) -> Result<(), ConfigurationError> {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("configuration.json");
        for suffix in 0..=1_000_u16 {
            let candidate = self.root.join(if suffix == 0 {
                format!("{name}.invalid")
            } else {
                format!("{name}.invalid-{suffix}")
            });
            if !candidate.exists() {
                fs::rename(path, candidate)?;
                return Ok(());
            }
        }
        Err(ConfigurationError::Invalid(
            "Too many invalid configuration backups exist.".into(),
        ))
    }
}

enum Candidate {
    Current(ConfigurationSnapshot),
    Migrated(ApplicationConfiguration),
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyConfigurationV0 {
    #[serde(default)]
    view_mode: Option<ViewMode>,
    #[serde(default)]
    caption_mode: Option<CaptionOutputPreference>,
    #[serde(default)]
    translation_target: Option<String>,
    #[serde(default)]
    overlay: Option<OverlaySettings>,
    #[serde(default)]
    visual: Option<VisualPreferences>,
    #[serde(default)]
    selected_speech_model_id: Option<String>,
}

impl LegacyConfigurationV0 {
    fn migrate(self) -> ApplicationConfiguration {
        let mut config = ApplicationConfiguration::default();
        if let Some(view_mode) = self.view_mode {
            config.view_mode = view_mode;
        }
        if let Some(caption_mode) = self.caption_mode {
            config.captions.output_mode = caption_mode;
        }
        if self.translation_target.as_deref() == Some("off") {
            config.captions.translation_target = None;
            config.captions.output_mode = CaptionOutputPreference::Original;
        } else if let Some(target) = self.translation_target {
            config.captions.translation_target = Some(target);
        }
        if let Some(overlay) = self.overlay {
            config.overlay = overlay;
        }
        if let Some(visual) = self.visual {
            config.visual = visual;
        }
        config.models.speech_model_id = self.selected_speech_model_id;
        config
    }
}

fn validate_overlay(settings: &OverlaySettings) -> Result<(), ConfigurationError> {
    if settings.font_family.trim().is_empty() || settings.font_family.len() > 256 {
        return Err(ConfigurationError::Invalid(
            "Caption font family must contain at most 256 bytes.".into(),
        ));
    }
    if !(18..=96).contains(&settings.font_size) {
        return Err(ConfigurationError::Invalid(
            "Caption size must be between 18 and 96 pixels.".into(),
        ));
    }
    if !(320..=1_600).contains(&settings.width) {
        return Err(ConfigurationError::Invalid(
            "Caption width must be between 320 and 1600 pixels.".into(),
        ));
    }
    if !(1..=4).contains(&settings.maximum_lines) {
        return Err(ConfigurationError::Invalid(
            "Maximum caption lines must be between 1 and 4.".into(),
        ));
    }
    if !(3..=60).contains(&settings.reading_time_seconds) {
        return Err(ConfigurationError::Invalid(
            "Caption reading time must be between 3 and 60 seconds.".into(),
        ));
    }
    if settings.fade_duration_ms > 5_000 {
        return Err(ConfigurationError::Invalid(
            "Caption fade duration must be at most 5 seconds.".into(),
        ));
    }
    if !settings.background_opacity.is_finite()
        || !(0.0..=1.0).contains(&settings.background_opacity)
    {
        return Err(ConfigurationError::Invalid(
            "Caption background opacity must be between 0 and 1.".into(),
        ));
    }
    if !is_hex_color(&settings.text_color) || !is_hex_color(&settings.translated_text_color) {
        return Err(ConfigurationError::Invalid(
            "Caption colors must be six-digit hex colors.".into(),
        ));
    }
    Ok(())
}

fn validate_language(value: &str, allow_auto: bool) -> Result<(), ConfigurationError> {
    if (allow_auto && value == "auto") || is_language_identifier(value) {
        return Ok(());
    }
    Err(ConfigurationError::Invalid(
        "A configured language identifier is invalid.".into(),
    ))
}

fn is_language_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
}

fn validate_opaque_id(kind: &str, value: &str) -> Result<(), ConfigurationError> {
    if value.trim().is_empty() || value.len() > 1_024 || value.chars().any(char::is_control) {
        return Err(ConfigurationError::Invalid(format!(
            "The configured {kind} identity is invalid."
        )));
    }
    Ok(())
}

fn validate_model_id(value: &str) -> Result<(), ConfigurationError> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ConfigurationError::Invalid(
            "A configured model identity is invalid.".into(),
        ));
    }
    Ok(())
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn configuration_filename(revision: u64) -> String {
    format!("{CONFIGURATION_PREFIX}{revision:020}{CONFIGURATION_SUFFIX}")
}

fn parse_configuration_revision(name: &str) -> Option<u64> {
    name.strip_prefix(CONFIGURATION_PREFIX)?
        .strip_suffix(CONFIGURATION_SUFFIX)?
        .parse()
        .ok()
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_round_trip() {
        let config = ApplicationConfiguration::default();
        config.validate().unwrap();
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(
            serde_json::from_str::<ApplicationConfiguration>(&json).unwrap(),
            config
        );
    }

    #[test]
    fn rejects_invalid_overlay_language_and_ids() {
        let mut config = ApplicationConfiguration::default();
        config.overlay.background_opacity = f32::NAN;
        assert!(config.validate().is_err());
        config = ApplicationConfiguration::default();
        config.visual.target_language = "".into();
        assert!(config.validate().is_err());
        config = ApplicationConfiguration::default();
        config.models.speech_model_id = Some("../model".into());
        assert!(config.validate().is_err());
    }

    #[test]
    fn repository_publishes_revisions_without_overwriting_the_last_good_file() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = ConfigurationRepository::new(temporary.path());
        let first = repository.load().unwrap().snapshot;
        assert_eq!(first.revision, 1);

        let mut config = first.config.clone();
        config.view_mode = ViewMode::Compact;
        let second = repository
            .save(first.revision, first.revision, config)
            .unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(repository.candidates().unwrap().len(), 2);

        let loaded = repository.load().unwrap().snapshot;
        assert_eq!(loaded.revision, 2);
        assert_eq!(loaded.config.view_mode, ViewMode::Compact);
    }

    #[test]
    fn stale_writes_cannot_replace_current_configuration() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = ConfigurationRepository::new(temporary.path());
        let current = repository.load().unwrap().snapshot;
        let error = repository
            .save(current.revision, 0, current.config)
            .unwrap_err();
        assert!(matches!(error, ConfigurationError::StaleRevision { .. }));
    }

    #[test]
    fn corrupt_latest_revision_is_quarantined_once_and_previous_revision_recovers() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = ConfigurationRepository::new(temporary.path());
        let first = repository.load().unwrap().snapshot;
        let second = repository
            .save(first.revision, first.revision, first.config)
            .unwrap();
        fs::write(
            temporary
                .path()
                .join(configuration_filename(second.revision)),
            b"{ definitely not json",
        )
        .unwrap();

        let recovered = repository.load().unwrap();
        assert!(recovered.recovered);
        assert_eq!(recovered.snapshot.revision, first.revision);
        assert!(recovered.snapshot.diagnostic.is_some());
        assert!(fs::read_dir(temporary.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".invalid")
        }));
    }

    #[test]
    fn version_zero_documents_migrate_to_the_current_schema() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(
            temporary.path().join(configuration_filename(7)),
            br#"{
              "schemaVersion": 0,
              "viewMode": "compact",
              "captionMode": "both",
              "translationTarget": "es",
              "selectedSpeechModelId": "speech-model"
            }"#,
        )
        .unwrap();
        let loaded = ConfigurationRepository::new(temporary.path())
            .load()
            .unwrap();

        assert_eq!(loaded.snapshot.revision, 8);
        assert_eq!(
            loaded.snapshot.config.schema_version,
            CONFIGURATION_SCHEMA_VERSION
        );
        assert_eq!(loaded.snapshot.config.view_mode, ViewMode::Compact);
        assert_eq!(
            loaded.snapshot.config.captions.output_mode,
            CaptionOutputPreference::Both
        );
        assert_eq!(
            loaded.snapshot.config.models.speech_model_id.as_deref(),
            Some("speech-model")
        );
    }

    #[test]
    fn abandoned_temporary_files_do_not_poison_launch() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(
            temporary
                .path()
                .join(".configuration-00000000000000000001.tmp"),
            b"partial",
        )
        .unwrap();
        let loaded = ConfigurationRepository::new(temporary.path())
            .load()
            .unwrap();
        assert_eq!(loaded.snapshot.revision, 1);
        assert_eq!(
            fs::read_dir(temporary.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
    }
}
