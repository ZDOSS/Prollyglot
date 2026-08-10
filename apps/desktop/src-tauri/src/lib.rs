use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use prollyglot_core::{
    CaptureEvent, CaptureSelection, CaptureSession, CaptureState, SourceSnapshot,
};
use serde::{Deserialize, Serialize};
use tauri::{
    Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalRect, PhysicalSize, State,
    WebviewWindow,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureStatus {
    state: CaptureState,
    peak: f32,
    dropped_frames: u64,
    source_label: Option<String>,
    message: Option<String>,
}

impl Default for CaptureStatus {
    fn default() -> Self {
        Self {
            state: CaptureState::Stopped,
            peak: 0.0,
            dropped_frames: 0,
            source_label: None,
            message: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlaySettings {
    font_family: String,
    font_size: u16,
    text_color: String,
    background_opacity: f32,
    width: u32,
    maximum_lines: u8,
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

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            font_family: r#""Segoe UI Variable", "Segoe UI", sans-serif"#.into(),
            font_size: 36,
            text_color: "#f4f6f5".into(),
            background_opacity: 0.75,
            width: 720,
            maximum_lines: 2,
            position: OverlayPosition::BottomCenter,
            click_through: true,
        }
    }
}

#[derive(Default)]
struct RuntimeState {
    control: Mutex<()>,
    session: Mutex<Option<Box<dyn CaptureSession>>>,
    status: Arc<Mutex<CaptureStatus>>,
    overlay_settings: Mutex<OverlaySettings>,
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

fn publish_status(app: &tauri::AppHandle, status: &Arc<Mutex<CaptureStatus>>, next: CaptureStatus) {
    *status.lock() = next.clone();
    if let Err(error) = app.emit("capture-status", next) {
        tracing::warn!(%error, "could not emit capture status");
    }
}

#[tauri::command]
fn source_snapshot() -> Result<SourceSnapshot, String> {
    prollyglot_audio_windows::source_snapshot().map_err(|error| {
        tracing::error!(%error, "could not enumerate Windows audio sources");
        error.to_string()
    })
}

#[tauri::command]
fn capture_status(state: State<'_, RuntimeState>) -> CaptureStatus {
    state.status.lock().clone()
}

#[tauri::command]
fn start_capture(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
    selection: CaptureSelection,
) -> Result<(), String> {
    let _control = state.control.lock();
    if matches!(
        state.status.lock().state,
        CaptureState::Failed | CaptureState::Stopped
    ) {
        drop(state.session.lock().take());
    }
    if state.session.lock().is_some() {
        return Err("A capture session is already running.".into());
    }

    tracing::info!(?selection, "starting capture session");

    let source_label = Some(selection.source_id().to_string());
    publish_status(
        &app,
        &state.status,
        CaptureStatus {
            state: CaptureState::Starting,
            peak: 0.0,
            dropped_frames: 0,
            source_label: source_label.clone(),
            message: None,
        },
    );

    let (event_sender, event_receiver) = crossbeam_channel::bounded(12);
    let session = match prollyglot_audio_windows::start_capture(selection, event_sender) {
        Ok(session) => session,
        Err(error) => {
            tracing::error!(%error, "could not start capture session");
            publish_status(
                &app,
                &state.status,
                CaptureStatus {
                    state: CaptureState::Failed,
                    peak: 0.0,
                    dropped_frames: 0,
                    source_label,
                    message: Some(error.to_string()),
                },
            );
            return Err(error.to_string());
        }
    };
    *state.session.lock() = Some(session);

    publish_status(
        &app,
        &state.status,
        CaptureStatus {
            state: CaptureState::Capturing,
            peak: 0.0,
            dropped_frames: 0,
            source_label: source_label.clone(),
            message: None,
        },
    );

    let app_for_events = app.clone();
    let status_for_events = Arc::clone(&state.status);
    let forwarder = std::thread::Builder::new()
        .name("capture-event-forwarder".into())
        .spawn(move || {
            let mut last_peak_publish = None::<Instant>;
            while let Ok(event) = event_receiver.recv() {
                if matches!(&event, CaptureEvent::Frame(_))
                    && last_peak_publish
                        .is_some_and(|last| last.elapsed() < Duration::from_millis(50))
                {
                    continue;
                }
                if matches!(&event, CaptureEvent::Frame(_)) {
                    last_peak_publish = Some(Instant::now());
                }
                let previous = status_for_events.lock().clone();
                let next = match event {
                    CaptureEvent::State(capture_state) => CaptureStatus {
                        state: capture_state,
                        ..previous
                    },
                    CaptureEvent::Frame(frame) => CaptureStatus {
                        state: if previous.state == CaptureState::Waiting {
                            CaptureState::Waiting
                        } else {
                            CaptureState::Capturing
                        },
                        peak: frame.peak,
                        ..previous
                    },
                    CaptureEvent::Warning(message) => CaptureStatus {
                        state: CaptureState::Waiting,
                        message: Some(message),
                        ..previous
                    },
                    CaptureEvent::FramesDropped { total } => {
                        tracing::warn!(total, "audio frames dropped because the pipeline was full");
                        CaptureStatus {
                            dropped_frames: total,
                            message: Some(format!(
                                "Audio processing fell behind; {total} packets were dropped."
                            )),
                            ..previous
                        }
                    }
                    CaptureEvent::Error(message) => {
                        tracing::error!(%message, "capture worker failed");
                        CaptureStatus {
                            state: CaptureState::Failed,
                            peak: 0.0,
                            message: Some(message),
                            ..previous
                        }
                    }
                };
                publish_status(&app_for_events, &status_for_events, next);
            }

            let runtime = app_for_events.state::<RuntimeState>();
            let _control = runtime.control.lock();
            if matches!(
                runtime.status.lock().state,
                CaptureState::Failed | CaptureState::Stopped
            ) {
                drop(runtime.session.lock().take());
            }
        })
        .map_err(|error| format!("Could not start the capture event forwarder: {error}"));
    if let Err(error) = forwarder {
        tracing::error!(%error, "could not start capture event forwarder");
        if let Some(mut session) = state.session.lock().take() {
            let _ = session.stop();
        }
        publish_status(
            &app,
            &state.status,
            CaptureStatus {
                state: CaptureState::Failed,
                message: Some(error.clone()),
                ..CaptureStatus::default()
            },
        );
        return Err(error);
    }

    Ok(())
}

#[tauri::command]
fn stop_capture(app: tauri::AppHandle, state: State<'_, RuntimeState>) -> Result<(), String> {
    let _control = state.control.lock();
    let Some(mut session) = state.session.lock().take() else {
        return Err("No capture session is running.".into());
    };

    let previous = state.status.lock().clone();
    publish_status(
        &app,
        &state.status,
        CaptureStatus {
            state: CaptureState::Stopping,
            peak: 0.0,
            message: None,
            ..previous
        },
    );

    session.stop().map_err(|error| {
        tracing::error!(%error, "could not stop capture session cleanly");
        error.to_string()
    })?;
    tracing::info!("capture session stopped");
    publish_status(&app, &state.status, CaptureStatus::default());
    Ok(())
}

#[tauri::command]
fn show_appearance_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("appearance")
        .ok_or("Appearance window is unavailable.")?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
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
    if !(0.0..=1.0).contains(&settings.background_opacity) {
        return Err("Background opacity must be between 0 and 1.".into());
    }
    if !settings.text_color.starts_with('#') || settings.text_color.len() != 7 {
        return Err("Text color must be a six-digit hex color.".into());
    }
    Ok(settings)
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
    let logical_height = (f64::from(settings.font_size) * 1.25 * f64::from(settings.maximum_lines)
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
fn show_overlay_preview(
    app: tauri::AppHandle,
    state: State<'_, RuntimeState>,
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

#[tauri::command]
fn hide_overlay_preview(app: tauri::AppHandle) -> Result<(), String> {
    let overlay = app
        .get_webview_window("overlay")
        .ok_or("Caption overlay is unavailable.")?;
    overlay.hide().map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeState::default())
        .setup(|app| initialize_logging(app))
        .invoke_handler(tauri::generate_handler![
            source_snapshot,
            start_capture,
            stop_capture,
            capture_status,
            show_appearance_window,
            update_overlay_settings,
            show_overlay_preview,
            hide_overlay_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Prollyglot");
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
