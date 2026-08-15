use std::sync::Arc;

use parking_lot::Mutex;
use prollyglot_application_runtime::{
    ApplicationError, ApplicationErrorCode, CaptionOutputMode, CaptionPresentationFrame,
    CaptionPresentationPhase, ErrorRecoverability, RecoveryAction, SessionId, SessionLifecycle,
    SessionMode, ipc,
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::RuntimeState;

const MAX_CAPTION_ENTRIES: usize = 4;
const MAX_CAPTION_TEXT_BYTES: usize = 16 * 1024;
const MAX_KEY_BYTES: usize = 1_024;
const MAX_LANGUAGE_BYTES: usize = 64;
const JS_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Default)]
pub struct CaptionPresentationRuntime {
    current: Arc<Mutex<Option<CaptionPresentationFrame>>>,
}

impl CaptionPresentationRuntime {
    pub fn begin_session(&self, session_id: SessionId, runtime_revision: u32) {
        *self.current.lock() = Some(cleared_frame(session_id, runtime_revision, 0));
    }

    pub fn current(&self) -> Option<CaptionPresentationFrame> {
        self.current.lock().clone()
    }

    pub fn clear_and_hide(&self, app: &AppHandle, session_id: SessionId, runtime_revision: u32) {
        let frame = {
            let mut current = self.current.lock();
            let presentation_revision = current
                .as_ref()
                .filter(|frame| frame.session_id == session_id)
                .map_or(0, |frame| frame.presentation_revision.saturating_add(1));
            let frame = cleared_frame(session_id, runtime_revision, presentation_revision);
            *current = Some(frame.clone());
            frame
        };
        if let Some(overlay) = app.get_webview_window("overlay") {
            if let Err(error) = overlay.emit(ipc::CAPTION_PRESENTATION_EVENT, frame) {
                tracing::warn!(%error, "could not clear caption presentation");
            }
            if let Err(error) = overlay.hide() {
                tracing::warn!(%error, "could not hide caption overlay");
            }
        }
    }
}

#[tauri::command]
pub fn update_caption_presentation(
    app: AppHandle,
    caller: WebviewWindow,
    state: State<'_, RuntimeState>,
    frame: CaptionPresentationFrame,
) -> Result<bool, ApplicationError> {
    require_main_webview(&caller, frame.session_id)?;
    validate_caption_frame(&frame).map_err(|message| invalid_frame(message, frame.session_id))?;
    let snapshot = state.supervisor.lock().snapshot();
    if snapshot.mode != Some(SessionMode::AudioCaptions)
        || snapshot.session_id != Some(frame.session_id)
        || !matches!(
            snapshot.lifecycle,
            SessionLifecycle::Starting | SessionLifecycle::Running | SessionLifecycle::Waiting
        )
        || frame.runtime_revision > snapshot.revision
    {
        return Ok(false);
    }
    let overlay = app.get_webview_window("overlay").ok_or_else(|| {
        ApplicationError::new(
            ApplicationErrorCode::WindowOperationFailed,
            "Caption presentation overlay is unavailable.",
            ErrorRecoverability::RestartRequired,
            RecoveryAction::RestartApplication,
        )
        .for_session(frame.session_id)
    })?;

    let previous = {
        let mut current = state.caption_presentation.current.lock();
        if current.as_ref().is_some_and(|current| {
            current.session_id != frame.session_id
                || frame.runtime_revision < current.runtime_revision
                || frame.presentation_revision <= current.presentation_revision
        }) {
            return Ok(false);
        }
        current.replace(frame.clone())
    };

    if let Err(error) = overlay.emit(ipc::CAPTION_PRESENTATION_EVENT, &frame) {
        *state.caption_presentation.current.lock() = previous;
        return Err(ApplicationError::new(
            ApplicationErrorCode::WindowOperationFailed,
            error.to_string(),
            ErrorRecoverability::Retryable,
            RecoveryAction::Retry,
        )
        .for_session(frame.session_id));
    }
    Ok(true)
}

fn require_main_webview(
    caller: &WebviewWindow,
    session_id: SessionId,
) -> Result<(), ApplicationError> {
    if caller.label() == "main" {
        return Ok(());
    }
    Err(ApplicationError::new(
        ApplicationErrorCode::Internal,
        "Only the main Prollyglot interface may publish caption presentation frames.",
        ErrorRecoverability::NotRecoverable,
        RecoveryAction::ReportIssue,
    )
    .for_session(session_id))
}

pub fn emit_current_caption(
    app: &AppHandle,
    runtime: &CaptionPresentationRuntime,
) -> Result<(), String> {
    let Some(frame) = runtime.current() else {
        return Ok(());
    };
    app.get_webview_window("overlay")
        .ok_or("Caption overlay is unavailable.")?
        .emit(ipc::CAPTION_PRESENTATION_EVENT, frame)
        .map_err(|error| error.to_string())
}

fn cleared_frame(
    session_id: SessionId,
    runtime_revision: u32,
    presentation_revision: u64,
) -> CaptionPresentationFrame {
    CaptionPresentationFrame {
        session_id,
        runtime_revision,
        presentation_revision,
        phase: CaptionPresentationPhase::Cleared,
        readable_at_ms: 0,
        mode: CaptionOutputMode::Original,
        target_language: None,
        entries: Vec::new(),
    }
}

fn validate_caption_frame(frame: &CaptionPresentationFrame) -> Result<(), String> {
    if frame.session_id.0 == 0 {
        return Err("Caption presentation requires an active session identifier.".into());
    }
    if frame.presentation_revision == 0
        || frame.presentation_revision > JS_MAX_SAFE_INTEGER
        || frame.readable_at_ms > JS_MAX_SAFE_INTEGER
    {
        return Err("Caption presentation revisions and timestamps must be safe integers.".into());
    }
    if frame.entries.len() > MAX_CAPTION_ENTRIES {
        return Err(format!(
            "Caption presentation contains {} rows; the maximum is {MAX_CAPTION_ENTRIES}.",
            frame.entries.len()
        ));
    }
    if frame.phase == CaptionPresentationPhase::Cleared
        && (!frame.entries.is_empty() || frame.readable_at_ms != 0)
    {
        return Err("A cleared caption presentation cannot contain rows or a timestamp.".into());
    }
    if frame.phase != CaptionPresentationPhase::Cleared
        && (frame.entries.is_empty() || frame.readable_at_ms == 0)
    {
        return Err(
            "A visible caption presentation requires rows and a readable timestamp.".into(),
        );
    }
    if frame.phase == CaptionPresentationPhase::Active
        && frame.entries.iter().all(|entry| entry.is_final)
    {
        return Err("An active caption presentation requires provisional speech.".into());
    }
    if frame.phase == CaptionPresentationPhase::Holding
        && frame.entries.iter().any(|entry| !entry.is_final)
    {
        return Err("A holding caption presentation cannot contain provisional speech.".into());
    }
    if frame.mode != CaptionOutputMode::Original
        && frame
            .target_language
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err("Translated caption output requires a target language.".into());
    }
    if frame
        .target_language
        .as_ref()
        .is_some_and(|language| language.len() > MAX_LANGUAGE_BYTES)
    {
        return Err("The caption target language identifier is too long.".into());
    }
    for entry in &frame.entries {
        if entry.key.is_empty() || entry.key.len() > MAX_KEY_BYTES {
            return Err("A caption row has an invalid identity key.".into());
        }
        if entry.source_language.trim().is_empty()
            || entry.source_language.len() > MAX_LANGUAGE_BYTES
        {
            return Err("A caption source language identifier is too long.".into());
        }
        if entry.original.trim().is_empty() || entry.original.len() > MAX_CAPTION_TEXT_BYTES {
            return Err("A caption row has invalid original text.".into());
        }
        if entry
            .translation
            .as_ref()
            .is_some_and(|text| text.trim().is_empty() || text.len() > MAX_CAPTION_TEXT_BYTES)
            || (entry.translation_pending && entry.translation.is_some())
        {
            return Err("A caption row has invalid translated text.".into());
        }
    }
    Ok(())
}

fn invalid_frame(message: String, session_id: SessionId) -> ApplicationError {
    ApplicationError::new(
        ApplicationErrorCode::ConfigurationInvalid,
        message,
        ErrorRecoverability::UserActionRequired,
        RecoveryAction::OpenSettings,
    )
    .for_session(session_id)
}

#[cfg(test)]
mod tests {
    use prollyglot_application_runtime::CaptionPresentationEntry;

    use super::*;

    fn frame() -> CaptionPresentationFrame {
        CaptionPresentationFrame {
            session_id: SessionId(3),
            runtime_revision: 10,
            presentation_revision: 2,
            phase: CaptionPresentationPhase::Holding,
            readable_at_ms: 1_750_000_000_000,
            mode: CaptionOutputMode::Both,
            target_language: Some("en".into()),
            entries: vec![CaptionPresentationEntry {
                key: "ja:1".into(),
                source_language: "ja".into(),
                original: "ニュース".into(),
                translation: Some("News".into()),
                translation_pending: false,
                is_final: true,
            }],
        }
    }

    #[test]
    fn caption_frames_require_consistent_visible_content() {
        assert!(validate_caption_frame(&frame()).is_ok());
        let mut invalid = frame();
        invalid.phase = CaptionPresentationPhase::Cleared;
        assert!(validate_caption_frame(&invalid).is_err());
        invalid.entries.clear();
        invalid.readable_at_ms = 0;
        assert!(validate_caption_frame(&invalid).is_ok());
    }

    #[test]
    fn caption_phase_matches_provisional_state() {
        let mut invalid = frame();
        invalid.phase = CaptionPresentationPhase::Active;
        assert!(validate_caption_frame(&invalid).is_err());

        invalid.entries[0].is_final = false;
        assert!(validate_caption_frame(&invalid).is_ok());
        invalid.phase = CaptionPresentationPhase::Holding;
        assert!(validate_caption_frame(&invalid).is_err());
    }
}
