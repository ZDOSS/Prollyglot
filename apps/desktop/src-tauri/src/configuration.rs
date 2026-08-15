use parking_lot::Mutex;
use prollyglot_application_runtime::{
    ApplicationConfiguration, ConfigurationSnapshot, UpdateConfigurationCommand, ipc,
};
use prollyglot_config::ConfigurationRepository;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::RuntimeState;

struct ConfigurationState {
    repository: ConfigurationRepository,
    snapshot: ConfigurationSnapshot,
}

#[derive(Default)]
pub struct ConfigurationRuntime {
    state: Mutex<Option<ConfigurationState>>,
}

impl ConfigurationRuntime {
    pub fn initialize(&self, app: &AppHandle) -> Result<ConfigurationSnapshot, String> {
        let root = app
            .path()
            .app_local_data_dir()
            .map_err(|error| {
                format!("Could not resolve the local configuration directory: {error}")
            })?
            .join("configuration");
        let repository = ConfigurationRepository::new(root);
        let loaded = repository.load().map_err(|error| error.to_string())?;
        if let Some(diagnostic) = &loaded.snapshot.diagnostic {
            tracing::warn!(%diagnostic, "local configuration recovered with a diagnostic");
        }
        let snapshot = loaded.snapshot;
        *self.state.lock() = Some(ConfigurationState {
            repository,
            snapshot: snapshot.clone(),
        });
        Ok(snapshot)
    }

    pub fn snapshot(&self) -> Result<ConfigurationSnapshot, String> {
        self.state
            .lock()
            .as_ref()
            .map(|state| state.snapshot.clone())
            .ok_or_else(|| "The local configuration has not initialized yet.".into())
    }

    pub fn update(
        &self,
        command: UpdateConfigurationCommand,
    ) -> Result<ConfigurationSnapshot, String> {
        let mut guard = self.state.lock();
        let state = guard
            .as_mut()
            .ok_or("The local configuration has not initialized yet.")?;
        if state.snapshot.revision != command.expected_revision {
            return Err(format!(
                "Configuration revision {} is stale; current revision is {}.",
                command.expected_revision, state.snapshot.revision
            ));
        }
        if state.snapshot.config.models != command.config.models {
            return Err(
                "Model selections must be changed through the model-management commands.".into(),
            );
        }
        let snapshot = state
            .repository
            .save(
                state.snapshot.revision,
                command.expected_revision,
                command.config,
            )
            .map_err(|error| error.to_string())?;
        state.snapshot = snapshot.clone();
        Ok(snapshot)
    }

    pub fn mutate(
        &self,
        mutate: impl FnOnce(&mut ApplicationConfiguration),
    ) -> Result<ConfigurationSnapshot, String> {
        let mut guard = self.state.lock();
        let state = guard
            .as_mut()
            .ok_or("The local configuration has not initialized yet.")?;
        let mut config = state.snapshot.config.clone();
        mutate(&mut config);
        let snapshot = state
            .repository
            .save(state.snapshot.revision, state.snapshot.revision, config)
            .map_err(|error| error.to_string())?;
        state.snapshot = snapshot.clone();
        Ok(snapshot)
    }
}

#[tauri::command]
pub fn configuration_snapshot(
    state: State<'_, RuntimeState>,
) -> Result<ConfigurationSnapshot, String> {
    state.configuration.snapshot()
}

#[tauri::command]
pub fn update_configuration(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, RuntimeState>,
    command: UpdateConfigurationCommand,
) -> Result<ConfigurationSnapshot, String> {
    require_configuration_writer(&window)?;
    let snapshot = state.configuration.update(command)?;
    publish(&app, &snapshot);
    crate::apply_configuration_snapshot(&app, &snapshot);
    Ok(snapshot)
}

pub fn set_speech_model(
    app: &AppHandle,
    runtime: &ConfigurationRuntime,
    model_id: String,
) -> Result<ConfigurationSnapshot, String> {
    let snapshot = runtime.mutate(|config| config.models.speech_model_id = Some(model_id))?;
    publish(app, &snapshot);
    Ok(snapshot)
}

pub fn publish(app: &AppHandle, snapshot: &ConfigurationSnapshot) {
    if let Err(error) = app.emit(ipc::CONFIGURATION_EVENT, snapshot) {
        tracing::warn!(%error, "could not emit the accepted configuration");
    }
}

fn require_configuration_writer(window: &WebviewWindow) -> Result<(), String> {
    match window.label() {
        "main" | "appearance" => Ok(()),
        label => Err(format!(
            "The {label} window is not allowed to change application configuration."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_uninitialized_runtime_fails_closed() {
        let runtime = ConfigurationRuntime::default();
        assert!(runtime.snapshot().is_err());
        assert!(
            runtime
                .mutate(|config| config.legacy_webview_imported = true)
                .is_err()
        );
    }
}
