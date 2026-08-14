mod audio;
mod models;
mod transcription;
mod visual;

use std::{fs, sync::Arc};

use parking_lot::Mutex;
use prollyglot_application_runtime::SessionSupervisor;
use prollyglot_transcript::{TranscriptSnapshot, TranscriptStore};
use serde::{Deserialize, Serialize};
use tauri::{
    Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalRect, PhysicalSize, State,
    WebviewWindow,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlaySettings {
    font_family: String,
    font_size: u16,
    text_color: String,
    translated_text_color: String,
    bilingual_layout: BilingualLayout,
    background_opacity: f32,
    width: u32,
    maximum_lines: u8,
    reading_time_seconds: u16,
    fade_duration_ms: u16,
    position: OverlayPosition,
    click_through: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum OverlayPosition {
    TopCenter,
    BottomCenter,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum BilingualLayout {
    Stacked,
    SideBySide,
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

#[derive(Default)]
struct RuntimeState {
    control: Mutex<()>,
    supervisor: Arc<Mutex<SessionSupervisor>>,
    audio: audio::AudioRuntime,
    transcript: Arc<Mutex<TranscriptStore>>,
    model: models::ModelRuntime,
    overlay_settings: Mutex<OverlaySettings>,
    visual: visual::VisualRuntime,
}

struct LoggingGuard {
    _worker: tracing_appender::non_blocking::WorkerGuard,
}

fn initialize_logging(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let log_directory = app.path().app_log_dir()?;
    fs::create_dir_all(&log_directory)?;
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("prollyglot")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_directory)?;
    let (writer, worker) = tracing_appender::non_blocking(file_appender);
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(true)
        .with_max_level(tracing::Level::INFO)
        .compact()
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    app.manage(LoggingGuard { _worker: worker });
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Prollyglot started");
    Ok(())
}

fn audio_session_active(state: &RuntimeState) -> bool {
    audio::is_active(state)
}

#[tauri::command]
fn transcript_snapshot(state: State<'_, RuntimeState>) -> TranscriptSnapshot {
    state.transcript.lock().snapshot().clone()
}

#[tauri::command]
fn clear_transcript(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<(), String> {
    let snapshot = {
        let mut transcript = state.transcript.lock();
        transcript.clear();
        transcript.snapshot().clone()
    };
    app.emit("transcript-update", snapshot)
        .map_err(|error| error.to_string())?;
    app.emit("overlay-caption", "")
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn show_appearance_window(app: tauri::AppHandle) -> Result<(), String> {
    // The caption overlay is an always-on-top window. Hide it while the
    // Appearance surface is open so a large non-click-through overlay cannot
    // cover the controls and trap the user in this window.
    if let Some(overlay) = app.get_webview_window("overlay") {
        if let Err(error) = overlay.set_ignore_cursor_events(true) {
            tracing::warn!(%error, "could not make the overlay ignore input before Appearance");
        }
        if let Err(error) = overlay.hide() {
            tracing::warn!(%error, "could not hide the overlay before Appearance");
        }
    }
    let window = app
        .get_webview_window("appearance")
        .ok_or("Appearance window is unavailable.")?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
fn close_appearance_window(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<(), String> {
    // Dismiss Appearance first. Any overlay restoration failure should be
    // reported, but it must never leave the settings window trapping input.
    let appearance = app
        .get_webview_window("appearance")
        .ok_or("Appearance window is unavailable.")?;
    appearance.hide().map_err(|error| error.to_string())?;

    let captions_are_running = audio::is_live(&state);
    if captions_are_running {
        if let Err(error) = restore_live_overlay(&app, &state) {
            tracing::warn!(%error, "Appearance closed but the live overlay could not be restored");
        }
    } else if let Some(overlay) = app.get_webview_window("overlay") {
        if let Err(error) = overlay.emit("overlay-caption", "") {
            tracing::warn!(%error, "could not clear overlay after closing Appearance");
        }
        if let Err(error) = overlay.hide() {
            tracing::warn!(%error, "could not hide overlay after closing Appearance");
        }
    }
    Ok(())
}

fn validated_settings(settings: OverlaySettings) -> Result<OverlaySettings, String> {
    if !(18..=96).contains(&settings.font_size) {
        return Err("Caption size must be between 18 and 96 pixels.".into());
    }
    if !(320..=1600).contains(&settings.width) {
        return Err("Caption width must be between 320 and 1600 pixels.".into());
    }
    if !(1..=4).contains(&settings.maximum_lines) {
        return Err("Maximum lines must be between 1 and 4.".into());
    }
    if !(3..=60).contains(&settings.reading_time_seconds) {
        return Err("Caption reading time must be between 3 and 60 seconds.".into());
    }
    if settings.fade_duration_ms > 5_000 {
        return Err("Caption fade duration must be at most 5 seconds.".into());
    }
    if !(0.0..=1.0).contains(&settings.background_opacity) {
        return Err("Background opacity must be between 0 and 1.".into());
    }
    if !is_hex_color(&settings.text_color) || !is_hex_color(&settings.translated_text_color) {
        return Err("Caption colors must be six-digit hex colors.".into());
    }
    Ok(settings)
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn configure_overlay_window(
    overlay: &WebviewWindow,
    settings: &OverlaySettings,
) -> Result<(), String> {
    overlay
        .set_ignore_cursor_events(settings.click_through)
        .map_err(|error| error.to_string())?;
    overlay
        .set_focusable(!settings.click_through)
        .map_err(|error| error.to_string())?;

    let monitor = match overlay
        .current_monitor()
        .map_err(|error| error.to_string())?
    {
        Some(monitor) => monitor,
        None => overlay
            .primary_monitor()
            .map_err(|error| error.to_string())?
            .ok_or("No monitor is available for the caption overlay.")?,
    };
    let scale_factor = monitor.scale_factor();
    let work_area = *monitor.work_area();
    let maximum_logical_width = (f64::from(work_area.size.width) / scale_factor - 32.0).max(320.0);
    let maximum_logical_height = (f64::from(work_area.size.height) / scale_factor - 32.0).max(80.0);
    let logical_width = (f64::from(settings.width) + 40.0).clamp(320.0, maximum_logical_width);
    let (bilingual_height, current_wrap_allowance) = match settings.bilingual_layout {
        BilingualLayout::Stacked => (2.0, 0.0),
        // Both columns wrap in full instead of ellipsizing history. Reserve two
        // visual lines per requested row plus room for a longer live pair; the
        // frontend drops only complete oldest pairs if content still exceeds
        // the available work area.
        BilingualLayout::SideBySide => (3.0, 3.0),
    };
    let logical_height = (f64::from(settings.font_size)
        * 1.25
        * (f64::from(settings.maximum_lines) * bilingual_height + current_wrap_allowance)
        + f64::from(settings.font_size)
            * 0.18
            * f64::from(settings.maximum_lines.saturating_sub(1))
        + 48.0)
        .clamp(80.0, maximum_logical_height);
    overlay
        .set_size(LogicalSize::new(logical_width, logical_height))
        .map_err(|error| error.to_string())?;

    let physical_size = PhysicalSize::new(
        (logical_width * scale_factor).round() as u32,
        (logical_height * scale_factor).round() as u32,
    );
    let margin = (24.0 * scale_factor).round() as i32;
    overlay
        .set_position(anchored_overlay_position(
            settings.position,
            work_area,
            physical_size,
            margin,
        ))
        .map_err(|error| error.to_string())
}

fn show_live_overlay(app: &tauri::AppHandle, state: &RuntimeState) -> Result<(), String> {
    show_overlay_with_caption(app, state, String::new())
}

fn restore_live_overlay(app: &tauri::AppHandle, state: &RuntimeState) -> Result<(), String> {
    let caption = transcription::overlay_caption(state.transcript.lock().snapshot());
    show_overlay_with_caption(app, state, caption)
}

fn show_overlay_with_caption(
    app: &tauri::AppHandle,
    state: &RuntimeState,
    caption: String,
) -> Result<(), String> {
    let overlay = app
        .get_webview_window("overlay")
        .ok_or("Caption overlay is unavailable.")?;
    let settings = state.overlay_settings.lock().clone();
    configure_overlay_window(&overlay, &settings)?;
    overlay
        .emit("overlay-settings", settings)
        .map_err(|error| error.to_string())?;
    overlay
        .emit("overlay-caption", caption)
        .map_err(|error| error.to_string())?;
    overlay.show().map_err(|error| error.to_string())
}

fn anchored_overlay_position(
    anchor: OverlayPosition,
    work_area: PhysicalRect<i32, u32>,
    overlay_size: PhysicalSize<u32>,
    margin: i32,
) -> PhysicalPosition<i32> {
    let origin_x = i64::from(work_area.position.x);
    let origin_y = i64::from(work_area.position.y);
    let width = i64::from(work_area.size.width);
    let height = i64::from(work_area.size.height);
    let overlay_width = i64::from(overlay_size.width);
    let overlay_height = i64::from(overlay_size.height);
    let margin = i64::from(margin.max(0));

    let left = origin_x + margin;
    let centered = origin_x + (width - overlay_width) / 2;
    let right = origin_x + width - overlay_width - margin;
    let top = origin_y + margin;
    let bottom = origin_y + height - overlay_height - margin;

    let (x, y) = match anchor {
        OverlayPosition::TopCenter => (centered, top),
        OverlayPosition::BottomCenter => (centered, bottom),
        OverlayPosition::BottomLeft => (left, bottom),
        OverlayPosition::BottomRight => (right, bottom),
    };
    let maximum_x = origin_x + (width - overlay_width).max(0);
    let maximum_y = origin_y + (height - overlay_height).max(0);
    PhysicalPosition::new(
        x.clamp(origin_x, maximum_x) as i32,
        y.clamp(origin_y, maximum_y) as i32,
    )
}

#[tauri::command]
fn update_overlay_settings(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
    settings: OverlaySettings,
) -> Result<(), String> {
    let settings = validated_settings(settings)?;
    let overlay = app
        .get_webview_window("overlay")
        .ok_or("Caption overlay is unavailable.")?;
    configure_overlay_window(&overlay, &settings)?;
    overlay
        .emit("overlay-settings", &settings)
        .map_err(|error| error.to_string())?;
    *state.overlay_settings.lock() = settings;
    Ok(())
}

#[tauri::command]
fn report_frontend_diagnostic(scope: String, message: String) {
    let scope: String = scope.trim().chars().take(80).collect();
    let message: String = message.trim().chars().take(2_000).collect();
    tracing::warn!(frontend_scope = %scope, %message, "frontend diagnostic");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeState::default())
        .setup(|app| {
            initialize_logging(app)?;
            let runtime = app.state::<RuntimeState>();
            models::initialize(app.handle(), &runtime.model);
            visual::initialize(app.handle(), &runtime.visual);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            audio::source_snapshot,
            audio::start_capture,
            audio::stop_capture,
            audio::capture_status,
            audio::runtime_bootstrap,
            transcript_snapshot,
            clear_transcript,
            models::model_status,
            models::select_speech_model,
            models::install_speech_model,
            models::remove_speech_model,
            show_appearance_window,
            close_appearance_window,
            update_overlay_settings,
            report_frontend_diagnostic,
            visual::visual_capabilities,
            visual::visual_source_snapshot,
            visual::visual_status,
            visual::visual_model_status,
            visual::show_visual_region_selector,
            visual::complete_visual_region_selection,
            visual::cancel_visual_region_selection,
            visual::install_visual_model,
            visual::remove_visual_model,
            visual::update_visual_overlay_output,
            visual::start_visual_translation,
            visual::stop_visual_translation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Prollyglot");
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic payload".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_settings_reject_unsafe_ranges() {
        let settings = OverlaySettings {
            background_opacity: 1.5,
            ..OverlaySettings::default()
        };
        assert!(validated_settings(settings).is_err());

        let settings = OverlaySettings {
            reading_time_seconds: 2,
            ..OverlaySettings::default()
        };
        assert!(validated_settings(settings).is_err());

        let settings = OverlaySettings {
            fade_duration_ms: 5_001,
            ..OverlaySettings::default()
        };
        assert!(validated_settings(settings).is_err());
    }

    #[test]
    fn overlay_anchor_uses_monitor_work_area() {
        let work_area = PhysicalRect {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1_920, 1_040),
        };
        let overlay = PhysicalSize::new(760, 160);

        assert_eq!(
            anchored_overlay_position(OverlayPosition::BottomCenter, work_area, overlay, 32),
            PhysicalPosition::new(580, 848)
        );
        assert_eq!(
            anchored_overlay_position(OverlayPosition::TopCenter, work_area, overlay, 32),
            PhysicalPosition::new(580, 32)
        );
    }

    #[test]
    fn overlay_anchor_supports_negative_monitor_coordinates() {
        let work_area = PhysicalRect {
            position: PhysicalPosition::new(-1_920, -120),
            size: PhysicalSize::new(1_920, 1_040),
        };
        let overlay = PhysicalSize::new(760, 160);

        assert_eq!(
            anchored_overlay_position(OverlayPosition::BottomRight, work_area, overlay, 32),
            PhysicalPosition::new(-792, 728)
        );
    }
}
