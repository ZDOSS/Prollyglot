use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use prollyglot_visual_pipeline::{
    DEFAULT_CAPTURE_FRAME_INTERVAL_MICROS, LatestFrameSender, PixelFormat, PixelRect, VisualFrame,
    latest_frame_channel,
};
use windows::Graphics::Capture::GraphicsCaptureSession;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
use windows_capture::{
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
    window::Window,
};

use crate::{
    PickedVisualSource, StartedVisualCapture, VisualCaptureCapabilities, VisualCaptureError,
    VisualCaptureEvent, VisualCaptureSelection, VisualCaptureSession, VisualSource,
    VisualSourceKind, VisualSourceSnapshot,
};

type HandlerError = String;

struct CaptureHandler {
    frame_sender: LatestFrameSender,
    event_sender: Sender<VisualCaptureEvent>,
    started_at: Instant,
    sequence: u64,
    replaced_frames: u64,
    contiguous_pixels: Vec<u8>,
    crop: Option<PixelRect>,
    geometry: CaptureGeometrySource,
}

struct CaptureFlags {
    source: PickedVisualSource,
    frame_sender: LatestFrameSender,
    event_sender: Sender<VisualCaptureEvent>,
    crop: Option<PixelRect>,
    geometry: CaptureGeometrySource,
}

#[derive(Clone, Copy)]
enum CaptureGeometrySource {
    Window(Window),
    Monitor(Monitor),
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = CaptureFlags;
    type Error = HandlerError;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let flags = context.flags;
        let _ = flags
            .event_sender
            .try_send(VisualCaptureEvent::Started(flags.source.clone()));
        Ok(Self {
            frame_sender: flags.frame_sender,
            event_sender: flags.event_sender,
            started_at: Instant::now(),
            sequence: 0,
            replaced_frames: 0,
            contiguous_pixels: Vec::new(),
            crop: flags.crop,
            geometry: flags.geometry,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        self.sequence = self.sequence.saturating_add(1);
        let (width, height, buffer) = if let Some(crop) = self.crop {
            if !crop.fits_within(frame.width(), frame.height()) {
                return Err(
                    "the selected region no longer fits inside the captured display".into(),
                );
            }
            let buffer = frame
                .buffer_crop(crop.x, crop.y, crop.x + crop.width, crop.y + crop.height)
                .map_err(|error| error.to_string())?;
            (crop.width, crop.height, buffer)
        } else {
            let width = frame.width();
            let height = frame.height();
            let buffer = frame.buffer().map_err(|error| error.to_string())?;
            (width, height, buffer)
        };
        let pixels = buffer
            .as_nopadding_buffer(&mut self.contiguous_pixels)
            .to_vec();
        let visual_frame = VisualFrame::new(
            self.sequence,
            self.started_at
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64,
            width,
            height,
            usize::try_from(width)
                .unwrap_or(usize::MAX)
                .saturating_mul(4),
            PixelFormat::Bgra8,
            pixels,
        )
        .map_err(|error| error.to_string())?;
        let queued = self
            .frame_sender
            .send(visual_frame)
            .map_err(|_| "the visual-processing worker stopped".to_owned())?;
        self.replaced_frames = self.replaced_frames.saturating_add(queued.replaced_frames);
        let (x, y, _, _) =
            capture_geometry(self.geometry, self.crop).map_err(|error| error.to_string())?;
        let _ = self.event_sender.try_send(VisualCaptureEvent::Frame {
            sequence: self.sequence,
            x,
            y,
            width,
            height,
            replaced_frames: self.replaced_frames,
        });
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        let _ = self.event_sender.try_send(VisualCaptureEvent::SourceClosed);
        Ok(())
    }
}

struct WindowsCaptureSession {
    control: Option<CaptureControl<CaptureHandler, HandlerError>>,
}

impl VisualCaptureSession for WindowsCaptureSession {
    fn stop(&mut self) -> Result<(), VisualCaptureError> {
        let Some(control) = self.control.take() else {
            return Ok(());
        };
        control
            .stop()
            .map_err(|error| VisualCaptureError::Stop(error.to_string()))
    }
}

impl Drop for WindowsCaptureSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub fn capabilities() -> VisualCaptureCapabilities {
    match GraphicsCaptureSession::IsSupported() {
        Ok(true) => VisualCaptureCapabilities {
            windows_graphics_capture: true,
            // The initial slice uses explicit window/display lists. A picker
            // owner tied to the Tauri HWND remains a separate compatibility gate.
            system_picker: false,
            desktop_duplication_experiment: false,
            message: None,
        },
        Ok(false) => VisualCaptureCapabilities {
            windows_graphics_capture: false,
            system_picker: false,
            desktop_duplication_experiment: false,
            message: Some("Windows Graphics Capture is unavailable on this system.".into()),
        },
        Err(error) => VisualCaptureCapabilities {
            windows_graphics_capture: false,
            system_picker: false,
            desktop_duplication_experiment: false,
            message: Some(format!("Could not query Windows Graphics Capture: {error}")),
        },
    }
}

pub fn source_snapshot() -> Result<VisualSourceSnapshot, VisualCaptureError> {
    let own_process_id = std::process::id();
    let mut windows = Window::enumerate()
        .map_err(|error| VisualCaptureError::Sources(error.to_string()))?
        .into_iter()
        .filter_map(|window| {
            if window.process_id().ok() == Some(own_process_id) {
                return None;
            }
            let label = window.title().ok()?.trim().to_owned();
            let rect = window.rect().ok()?;
            let width = u32::try_from(rect.right - rect.left).ok()?;
            let height = u32::try_from(rect.bottom - rect.top).ok()?;
            if label.is_empty() || width == 0 || height == 0 {
                return None;
            }
            Some(VisualSource {
                id: window_id(window),
                kind: VisualSourceKind::ApplicationWindow,
                label,
                x: rect.left,
                y: rect.top,
                width,
                height,
            })
        })
        .collect::<Vec<_>>();
    windows.sort_by_key(|source| source.label.to_lowercase());

    let primary = Monitor::primary().ok();
    let mut displays = Monitor::enumerate()
        .map_err(|error| VisualCaptureError::Sources(error.to_string()))?
        .into_iter()
        .filter_map(|monitor| {
            let (x, y, width, height) = monitor_geometry(monitor).ok()?;
            let mut label = monitor
                .name()
                .or_else(|_| monitor.device_name())
                .unwrap_or_else(|_| "Display".into());
            if primary == Some(monitor) {
                label.push_str(" · Primary");
            }
            Some(VisualSource {
                id: monitor_id(monitor),
                kind: VisualSourceKind::Display,
                label,
                x,
                y,
                width,
                height,
            })
        })
        .collect::<Vec<_>>();
    displays.sort_by_key(|source| source.label.to_lowercase());
    Ok(VisualSourceSnapshot { windows, displays })
}

pub fn start_capture(
    selection: VisualCaptureSelection,
) -> Result<StartedVisualCapture, VisualCaptureError> {
    if !GraphicsCaptureSession::IsSupported()
        .map_err(|error| VisualCaptureError::Start(error.to_string()))?
    {
        return Err(VisualCaptureError::Unsupported);
    }
    match selection {
        VisualCaptureSelection::ApplicationWindow { source_id } => {
            let window = resolve_window(&source_id)?;
            let source = picked_window(window)?;
            start_item(window, source, None, CaptureGeometrySource::Window(window))
        }
        VisualCaptureSelection::Display { source_id } => {
            let monitor = resolve_monitor(&source_id)?;
            let source = picked_monitor(monitor, None)?;
            start_item(
                monitor,
                source,
                None,
                CaptureGeometrySource::Monitor(monitor),
            )
        }
        VisualCaptureSelection::Region { display_id, region } => {
            let monitor = resolve_monitor(&display_id)?;
            let source = picked_monitor(monitor, Some(region))?;
            start_item(
                monitor,
                source,
                Some(region),
                CaptureGeometrySource::Monitor(monitor),
            )
        }
    }
}

fn start_item<T>(
    item: T,
    source: PickedVisualSource,
    crop: Option<PixelRect>,
    geometry: CaptureGeometrySource,
) -> Result<StartedVisualCapture, VisualCaptureError>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    let (frame_sender, frames) = latest_frame_channel();
    let (event_sender, events) = crossbeam_channel::bounded(32);
    let settings = Settings::new(
        item,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Exclude,
        MinimumUpdateIntervalSettings::Custom(Duration::from_micros(
            DEFAULT_CAPTURE_FRAME_INTERVAL_MICROS,
        )),
        DirtyRegionSettings::ReportOnly,
        ColorFormat::Bgra8,
        CaptureFlags {
            source: source.clone(),
            frame_sender,
            event_sender,
            crop,
            geometry,
        },
    );
    let control = CaptureHandler::start_free_threaded(settings)
        .map_err(|error| VisualCaptureError::Start(error.to_string()))?;
    Ok(StartedVisualCapture {
        source,
        frames,
        events,
        session: Box::new(WindowsCaptureSession {
            control: Some(control),
        }),
    })
}

fn picked_window(window: Window) -> Result<PickedVisualSource, VisualCaptureError> {
    let rect = window
        .rect()
        .map_err(|error| VisualCaptureError::Sources(error.to_string()))?;
    let width = u32::try_from(rect.right - rect.left)
        .map_err(|_| VisualCaptureError::Sources("window width is invalid".into()))?;
    let height = u32::try_from(rect.bottom - rect.top)
        .map_err(|_| VisualCaptureError::Sources("window height is invalid".into()))?;
    Ok(PickedVisualSource {
        label: window
            .title()
            .map_err(|error| VisualCaptureError::Sources(error.to_string()))?,
        x: rect.left,
        y: rect.top,
        width,
        height,
    })
}

fn picked_monitor(
    monitor: Monitor,
    region: Option<PixelRect>,
) -> Result<PickedVisualSource, VisualCaptureError> {
    let (monitor_x, monitor_y, monitor_width, monitor_height) = monitor_geometry(monitor)?;
    if region.is_some_and(|crop| !crop.fits_within(monitor_width, monitor_height)) {
        return Err(VisualCaptureError::Sources(
            "the selected region is outside the display".into(),
        ));
    }
    let mut label = monitor
        .name()
        .or_else(|_| monitor.device_name())
        .unwrap_or_else(|_| "Display".into());
    if region.is_some() {
        label.push_str(" · Region");
    }
    Ok(PickedVisualSource {
        label,
        x: monitor_x.saturating_add(region.map_or(0, |crop| crop.x) as i32),
        y: monitor_y.saturating_add(region.map_or(0, |crop| crop.y) as i32),
        width: region.map_or(monitor_width, |crop| crop.width),
        height: region.map_or(monitor_height, |crop| crop.height),
    })
}

fn resolve_window(source_id: &str) -> Result<Window, VisualCaptureError> {
    Window::enumerate()
        .map_err(|error| VisualCaptureError::Sources(error.to_string()))?
        .into_iter()
        .find(|window| window_id(*window) == source_id)
        .ok_or_else(|| {
            VisualCaptureError::Sources("the selected window is no longer available".into())
        })
}

fn resolve_monitor(source_id: &str) -> Result<Monitor, VisualCaptureError> {
    Monitor::enumerate()
        .map_err(|error| VisualCaptureError::Sources(error.to_string()))?
        .into_iter()
        .find(|monitor| monitor_id(*monitor) == source_id)
        .ok_or_else(|| {
            VisualCaptureError::Sources("the selected display is no longer available".into())
        })
}

fn window_id(window: Window) -> String {
    format!("window:{:x}", window.as_raw_hwnd() as usize)
}

fn monitor_id(monitor: Monitor) -> String {
    format!("display:{:x}", monitor.as_raw_hmonitor() as usize)
}

fn capture_geometry(
    source: CaptureGeometrySource,
    crop: Option<PixelRect>,
) -> Result<(i32, i32, u32, u32), VisualCaptureError> {
    match source {
        CaptureGeometrySource::Window(window) => {
            let rect = window
                .rect()
                .map_err(|error| VisualCaptureError::Sources(error.to_string()))?;
            let width = u32::try_from(rect.right - rect.left)
                .map_err(|_| VisualCaptureError::Sources("window width is invalid".into()))?;
            let height = u32::try_from(rect.bottom - rect.top)
                .map_err(|_| VisualCaptureError::Sources("window height is invalid".into()))?;
            Ok((rect.left, rect.top, width, height))
        }
        CaptureGeometrySource::Monitor(monitor) => {
            let (x, y, width, height) = monitor_geometry(monitor)?;
            if let Some(crop) = crop {
                if !crop.fits_within(width, height) {
                    return Err(VisualCaptureError::Sources(
                        "the selected region is outside the display".into(),
                    ));
                }
                Ok((
                    x.saturating_add(crop.x as i32),
                    y.saturating_add(crop.y as i32),
                    crop.width,
                    crop.height,
                ))
            } else {
                Ok((x, y, width, height))
            }
        }
    }
}

fn monitor_geometry(monitor: Monitor) -> Result<(i32, i32, u32, u32), VisualCaptureError> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    let succeeded = unsafe {
        GetMonitorInfoW(
            HMONITOR(monitor.as_raw_hmonitor()),
            &mut info as *mut MONITORINFO,
        )
    };
    if !succeeded.as_bool() {
        return Err(VisualCaptureError::Sources(
            "Windows could not read the selected display geometry".into(),
        ));
    }
    let width = u32::try_from(info.rcMonitor.right - info.rcMonitor.left)
        .map_err(|_| VisualCaptureError::Sources("display width is invalid".into()))?;
    let height = u32::try_from(info.rcMonitor.bottom - info.rcMonitor.top)
        .map_err(|_| VisualCaptureError::Sources("display height is invalid".into()))?;
    Ok((info.rcMonitor.left, info.rcMonitor.top, width, height))
}
