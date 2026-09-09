//! Native Ubuntu selection and conservative X11/XWayland overlay placement.
//! Preview pixels stay inside GTK/Cairo and never enter the webview or IPC.
use crate::{
    visual_capture::{PickedVisualSource, StartError},
    visual_geometry::{LogicalRect, drag_region, preview_point},
};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use gtk::{cairo, gdk, glib, prelude::*};
use parking_lot::Mutex;
use prollyglot_application_runtime::{CancellationToken, VisualPresentationFrame};
use prollyglot_visual_pipeline::{PixelRect, VisualFrame};
use prollyglot_visual_pipewire::{CaptureEvent, FrameGeometry};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{Emitter, Manager};

pub fn select_region(
    app: &tauri::AppHandle,
    frame: &VisualFrame,
    cancellation: &CancellationToken,
    events: &Receiver<CaptureEvent>,
    geometry: FrameGeometry,
) -> Result<PixelRect, StartError> {
    crate::visual::region_selection_started(app);
    let (tx, rx) = crossbeam_channel::bounded(1);
    let closed = Arc::new(AtomicBool::new(false));
    let ui_closed = Arc::clone(&closed);
    let ui_app = app.clone();
    let image = (
        frame.width,
        frame.height,
        frame.stride,
        frame.pixels().to_vec(),
    );
    app.run_on_main_thread(move || {
        if ui_closed.load(Ordering::Acquire) {
            return;
        }
        if let Err(error) = open_selector(&ui_app, image, tx.clone(), ui_closed) {
            let _ = tx.try_send(Err(StartError::Failed(error)));
        }
    })
    .map_err(|e| StartError::Failed(e.to_string()))?;
    let result = loop {
        if cancellation.is_cancelled() {
            break Err(StartError::Cancelled);
        }
        if let Err(error) = crate::visual_portal::check_startup_events(events, geometry) {
            break Err(error);
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => break result,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break Err(StartError::Cancelled),
        }
    };
    closed.store(true, Ordering::Release);
    result
}

fn selected(spins: &[gtk::SpinButton; 4]) -> PixelRect {
    PixelRect {
        x: spins[0].value_as_int() as u32,
        y: spins[1].value_as_int() as u32,
        width: spins[2].value_as_int() as u32,
        height: spins[3].value_as_int() as u32,
    }
}

fn entered_region(spins: &[gtk::SpinButton]) -> Option<PixelRect> {
    let value = |index: usize| spins[index].text().trim().parse::<u32>().ok();
    Some(PixelRect {
        x: value(0)?,
        y: value(1)?,
        width: value(2)?,
        height: value(3)?,
    })
}

fn open_selector(
    app: &tauri::AppHandle,
    image: (u32, u32, usize, Vec<u8>),
    tx: crossbeam_channel::Sender<Result<PixelRect, StartError>>,
    closed: Arc<AtomicBool>,
) -> Result<(), String> {
    let (width, height, stride, pixels) = image;
    let surface = cairo::ImageSurface::create_for_data(
        pixels,
        cairo::Format::Rgb24,
        width as i32,
        height as i32,
        stride as i32,
    )
    .map_err(|e| e.to_string())?;
    let dialog = gtk::Dialog::builder()
        .title("Choose screen region — Prollyglot")
        .default_width(960)
        .default_height(640)
        .destroy_with_parent(true)
        .build();
    if let Some(main) = app.get_webview_window("main") {
        dialog.set_transient_for(Some(&main.gtk_window().map_err(|e| e.to_string())?));
    }
    dialog.add_button("_Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("_Use region", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Accept);
    let content = dialog.content_area();
    content.set_spacing(10);
    content.set_border_width(12);
    let help = gtk::Label::new(Some(
        "Drag around the text in this still preview, or edit the pixel coordinates below.\nUse region starts live capture of that area. Esc cancels.",
    ));
    help.set_line_wrap(true);
    content.pack_start(&help, false, false, 0);
    let area = gtk::DrawingArea::new();
    area.set_size_request(320, 200);
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.add_events(
        gdk::EventMask::BUTTON_PRESS_MASK
            | gdk::EventMask::BUTTON_RELEASE_MASK
            | gdk::EventMask::POINTER_MOTION_MASK,
    );
    content.pack_start(&area, true, true, 0);
    let coordinates = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let spins: [gtk::SpinButton; 4] = std::array::from_fn(|index| {
        let limit = if index % 2 == 0 { width } else { height };
        let spin = gtk::SpinButton::with_range(0.0, f64::from(limit), 1.0);
        spin.set_numeric(true);
        spin.set_width_chars(5);
        spin.set_value(if index >= 2 { f64::from(limit) } else { 0.0 });
        let label = gtk::Label::new(Some(["_X", "_Y", "_Width", "_Height"][index]));
        label.set_use_underline(true);
        label.set_mnemonic_widget(Some(&spin));
        coordinates.pack_start(&label, false, false, 0);
        coordinates.pack_start(&spin, true, true, 0);
        spin
    });
    content.pack_start(&coordinates, false, false, 0);
    let dimensions = gtk::Label::new(Some(&format!(
        "{width} × {height} captured pixels. Select at least 80 × 60 pixels."
    )));
    dimensions.set_line_wrap(true);
    content.pack_start(&dimensions, false, false, 0);
    for spin in &spins {
        let weak_area = area.downgrade();
        let weak_dialog = dialog.downgrade();
        let weak_spins = spins.each_ref().map(|s| s.downgrade());
        spin.connect_changed(move |_| {
            if let Some(spins) = weak_spins
                .iter()
                .map(|s| s.upgrade())
                .collect::<Option<Vec<_>>>()
            {
                let valid = entered_region(&spins).is_some_and(|region| {
                    region.fits_within(width, height) && region.width >= 80 && region.height >= 60
                });
                if let Some(dialog) = weak_dialog.upgrade() {
                    dialog.set_response_sensitive(gtk::ResponseType::Accept, valid);
                }
            }
            if let Some(area) = weak_area.upgrade() {
                area.queue_draw();
            }
        });
    }
    let draw_spins = spins.clone();
    area.connect_draw(move |area, ctx| {
        let (aw, ah) = (
            f64::from(area.allocated_width()),
            f64::from(area.allocated_height()),
        );
        let scale = (aw / f64::from(width)).min(ah / f64::from(height));
        ctx.set_source_rgb(0.06, 0.08, 0.1);
        let _ = ctx.paint();
        let _ = ctx.save();
        ctx.translate(
            (aw - f64::from(width) * scale) / 2.0,
            (ah - f64::from(height) * scale) / 2.0,
        );
        ctx.scale(scale, scale);
        let _ = ctx.set_source_surface(&surface, 0.0, 0.0);
        let _ = ctx.paint();
        let r = entered_region(&draw_spins).unwrap_or_else(|| selected(&draw_spins));
        ctx.rectangle(
            f64::from(r.x),
            f64::from(r.y),
            f64::from(r.width),
            f64::from(r.height),
        );
        ctx.set_source_rgba(0.2, 0.85, 1.0, 0.18);
        let _ = ctx.fill_preserve();
        ctx.set_source_rgb(0.3, 0.9, 1.0);
        ctx.set_line_width(2.0 / scale);
        let _ = ctx.stroke();
        let _ = ctx.restore();
        glib::Propagation::Stop
    });
    let origin = Rc::new(Cell::new(None));
    let pressed = Rc::clone(&origin);
    area.connect_button_press_event(move |area, event| {
        if event.button() == 1 {
            pressed.set(preview_point(
                width,
                height,
                (
                    f64::from(area.allocated_width()),
                    f64::from(area.allocated_height()),
                ),
                event.position(),
                false,
            ));
        }
        glib::Propagation::Stop
    });
    let moved = Rc::clone(&origin);
    let drag_spins = spins.clone();
    area.connect_motion_notify_event(move |area, event| {
        if let Some(start) = moved.get()
            && let Some(end) = preview_point(
                width,
                height,
                (
                    f64::from(area.allocated_width()),
                    f64::from(area.allocated_height()),
                ),
                event.position(),
                true,
            )
        {
            let r = drag_region(start, end);
            for (spin, value) in drag_spins.iter().zip([r.x, r.y, r.width, r.height]) {
                spin.set_value(f64::from(value));
            }
        }
        glib::Propagation::Stop
    });
    let release_spins = spins.clone();
    area.connect_button_release_event(move |area, event| {
        if event.button() == 1
            && let Some(start) = origin.take()
            && let Some(end) = preview_point(
                width,
                height,
                (
                    f64::from(area.allocated_width()),
                    f64::from(area.allocated_height()),
                ),
                event.position(),
                true,
            )
        {
            // Apply the release position even when GTK coalesces the last
            // motion event or the pointer finishes outside the preview.
            let region = drag_region(start, end);
            for (spin, value) in
                release_spins
                    .iter()
                    .zip([region.x, region.y, region.width, region.height])
            {
                spin.set_value(f64::from(value));
            }
        }
        glib::Propagation::Stop
    });
    let done = Arc::clone(&closed);
    let destroyed = Rc::new(Cell::new(false));
    let on_destroy = Rc::clone(&destroyed);
    let destroy_closed = Arc::clone(&closed);
    dialog.connect_destroy(move |_| {
        on_destroy.set(true);
        destroy_closed.store(true, Ordering::Release);
    });
    let response_destroyed = Rc::clone(&destroyed);
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            // Keyboard confirmation can happen before the last entry loses
            // focus. Commit its text before validating the requested crop.
            for spin in &spins {
                spin.update();
            }
        }
        let region = selected(&spins);
        if response == gtk::ResponseType::Accept
            && (!region.fits_within(width, height) || region.width < 80 || region.height < 60)
        {
            return;
        }
        if response_destroyed.replace(true) {
            return;
        }
        let result = if response == gtk::ResponseType::Accept {
            Ok(region)
        } else {
            Err(StartError::Cancelled)
        };
        let _ = tx.try_send(result);
        done.store(true, Ordering::Release);
        // All signal handlers use children only while the dialog lives. The
        // timer releases its owning reference after response/destruction.
        unsafe {
            dialog.destroy();
        }
    });
    let pending_dialog = dialog.clone();
    glib::timeout_add_local(Duration::from_millis(20), move || {
        if closed.load(Ordering::Acquire) {
            if !destroyed.get() {
                pending_dialog.response(gtk::ResponseType::Cancel);
            }
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
    dialog.show_all();
    tracing::info!(width, height, "Native screen region preview opened");
    Ok(())
}

fn monitors(display: &gdk::Display) -> Vec<(LogicalRect, i32)> {
    (0..display.n_monitors())
        .filter_map(|index| display.monitor(index))
        .map(|monitor| {
            let r = monitor.geometry();
            (
                LogicalRect {
                    x: r.x(),
                    y: r.y(),
                    width: r.width(),
                    height: r.height(),
                },
                monitor.scale_factor(),
            )
        })
        .collect()
}

fn reader(window: &gtk::ApplicationWindow) {
    let was_anchored = !window.is_decorated();
    window.set_decorated(true);
    window.set_skip_taskbar_hint(false);
    window.set_keep_above(false);
    window.set_accept_focus(true);
    window.set_focus_on_map(false);
    window.set_resizable(true);
    window.set_opacity(1.0);
    if was_anchored {
        window.resize(560, 360);
        window.set_position(gtk::WindowPosition::Center);
    }
    window.input_shape_combine_region(None);
}

pub fn configure_overlay(
    app: &tauri::AppHandle,
    source: &PickedVisualSource,
    output: &Arc<Mutex<VisualPresentationFrame>>,
    show: bool,
) -> Result<(), String> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let app_ui = app.clone();
    let source = source.clone();
    let output = Arc::clone(output);
    let requested_session = output.lock().session_id;
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            let snapshot = app_ui.state::<crate::RuntimeState>().supervisor.lock().snapshot();
            if snapshot.session_id != Some(requested_session)
                || !matches!(snapshot.lifecycle, prollyglot_application_runtime::SessionLifecycle::Starting
                    | prollyglot_application_runtime::SessionLifecycle::Running
                    | prollyglot_application_runtime::SessionLifecycle::Waiting) {
                return Ok(());
            }
            let overlay = app_ui.get_webview_window("visual-overlay").ok_or("Screen translations are unavailable.")?;
            let window = overlay.gtk_window().map_err(|e| e.to_string())?;
            let display = WidgetExt::display(&window);
            let layout = monitors(&display);
            let anchor = if display.type_().name() == "GdkX11Display" {
                source.portal.as_ref().and_then(|geometry| geometry.anchor(&layout.iter().map(|(rect, _)| *rect).collect::<Vec<_>>()))
            } else { None };
            crate::visual::set_linux_output_mode(&app_ui, &output, anchor.is_some());
            if let Some(anchor) = anchor {
                window.set_opacity(0.0);
                window.set_position(gtk::WindowPosition::None);
                window.set_decorated(false);
                window.set_skip_taskbar_hint(true);
                window.set_keep_above(true);
                window.set_accept_focus(false);
                window.set_focus_on_map(false);
                window.set_resizable(true);
                window.resize(anchor.width, anchor.height);
                window.move_(anchor.x, anchor.y);
                window.input_shape_combine_region(Some(&cairo::Region::create()));
            } else { reader(&window); }
            overlay.emit(prollyglot_application_runtime::ipc::VISUAL_PRESENTATION_EVENT, output.lock().clone()).map_err(|e| e.to_string())?;
            if show { window.show(); } else { window.hide(); }
            if let Some(anchor) = anchor.filter(|_| show) {
                // Move again after map, then verify what the WM actually did.
                window.move_(anchor.x, anchor.y);
                let session_id = output.lock().session_id;
                let weak = window.downgrade();
                glib::timeout_add_local(Duration::from_millis(400), move || {
                    let current = output.lock();
                    if current.session_id != session_id || !current.anchored { return glib::ControlFlow::Break; }
                    drop(current);
                    let Some(window) = weak.upgrade() else { return glib::ControlFlow::Break; };
                    if !window.is_visible() { return glib::ControlFlow::Break; }
                    if monitors(&display) != layout || window.position() != (anchor.x, anchor.y)
                        || window.size() != (anchor.width, anchor.height) {
                        window.hide();
                        crate::visual::set_linux_output_mode(&app_ui, &output, false);
                        reader(&window);
                        let _ = overlay.emit(prollyglot_application_runtime::ipc::VISUAL_PRESENTATION_EVENT, output.lock().clone());
                        window.show();
                        tracing::info!("Desktop geometry changed or overlay placement was declined; using the screen translation reader");
                        return glib::ControlFlow::Break;
                    }
                    window.set_opacity(1.0);
                    glib::ControlFlow::Continue
                });
            }
            Ok(())
        })();
        let _ = tx.send(result);
    }).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}
