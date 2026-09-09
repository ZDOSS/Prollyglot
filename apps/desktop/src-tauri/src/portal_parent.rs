//! Keep a real picker parent alive for one sharing session. GTK/Wayland work
//! stays on the UI thread; the capture worker owns only an identifier and a
//! release flag. No compositor response can make Start or Stop wait forever.

use std::{
    ffi::{CStr, c_char, c_void},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam_channel::{Sender, bounded};
use gtk::{glib, prelude::*};
use prollyglot_application_runtime::CancellationToken;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::Manager;

use crate::visual_capture::StartError;

const POLL: Duration = Duration::from_millis(20);
const EXPORT_TIMEOUT: Duration = Duration::from_secs(1);

pub struct PortalParent {
    identifier: String,
    released: Arc<AtomicBool>,
}

impl PortalParent {
    /// Called on the blocking capture startup worker. An unavailable exporter
    /// uses the portal's documented empty-parent form, never a wl_surface ID.
    pub fn export(
        app: &tauri::AppHandle,
        cancellation: &CancellationToken,
    ) -> Result<Self, StartError> {
        let mut parent = Self {
            identifier: String::new(),
            released: Arc::new(AtomicBool::new(false)),
        };
        let released = Arc::clone(&parent.released);
        let (tx, rx) = bounded(1);
        let app_ui = app.clone();
        app.run_on_main_thread(move || {
            if !released.load(Ordering::Acquire) {
                export_on_main(&app_ui, tx, released);
            }
        })
        .map_err(|error| StartError::Failed(error.to_string()))?;
        let started = Instant::now();
        loop {
            if cancellation.is_cancelled() {
                return Err(StartError::Cancelled);
            }
            match rx.recv_timeout(POLL) {
                Ok(identifier) => {
                    parent.identifier = identifier;
                    return Ok(parent);
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Ok(parent),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            }
            if started.elapsed() >= EXPORT_TIMEOUT {
                // Stop waiting and release even if a callback arrives later.
                parent.release();
                tracing::warn!("Picker parent export timed out; using an unparented picker");
                return Ok(parent);
            }
        }
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub fn release(&self) {
        self.released.store(true, Ordering::Release);
    }
}

impl Drop for PortalParent {
    fn drop(&mut self) {
        self.release();
    }
}

fn export_on_main(app: &tauri::AppHandle, tx: Sender<String>, released: Arc<AtomicBool>) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let Ok(handle) = main.window_handle() else {
        return;
    };
    match handle.as_raw() {
        RawWindowHandle::Xlib(handle) if handle.window != 0 => {
            let _ = tx.try_send(format!("x11:{:x}", handle.window));
            return;
        }
        RawWindowHandle::Xcb(handle) => {
            let _ = tx.try_send(format!("x11:{:x}", handle.window.get()));
            return;
        }
        RawWindowHandle::Wayland(_) => {}
        _ => return,
    }
    let Ok(main) = main.gtk_window() else {
        return;
    };
    let Some(window) = main.window().filter(|window| !window.is_destroyed()) else {
        return;
    };
    let display = window.display();
    if display.type_().name() != "GdkWaylandDisplay" || !main.is_mapped() {
        return;
    }
    // SAFETY: backend type and a live, mapped window were checked above. Both
    // registry queries and export run on GTK's main thread. GTK 3.24 supports
    // xdg_foreign v1/v2; absence is a normal fallback, not a capture failure.
    let supported = unsafe {
        [c"zxdg_exporter_v2", c"zxdg_exporter_v1"]
            .iter()
            .any(|name| {
                gdk_wayland_sys::gdk_wayland_display_query_registry(
                    display.as_ptr().cast(),
                    name.as_ptr(),
                ) != 0
            })
    };
    if !supported {
        tracing::info!("Desktop cannot export a picker parent; using an unparented picker");
        return;
    }
    let data = Box::into_raw(Box::new(tx)).cast::<c_void>();
    // SAFETY: GDK owns data only on success and calls destroy_sender once when
    // it releases the callback. On failure we retain and free the allocation.
    let exported = unsafe {
        gdk_wayland_sys::gdk_wayland_window_export_handle(
            window.as_ptr().cast(),
            Some(exported_handle),
            data,
            Some(destroy_sender),
        ) != 0
    };
    if !exported {
        unsafe { destroy_sender(data) };
        return;
    }
    glib::timeout_add_local(POLL, move || {
        if released.load(Ordering::Acquire) || window.is_destroyed() {
            // SAFETY: exactly one successful export and execution on the GTK
            // thread. This owning GDK reference retains the export even if the
            // native surface was destroyed; unexport before dropping it.
            unsafe {
                gdk_wayland_sys::gdk_wayland_window_unexport_handle(window.as_ptr().cast());
            }
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
}

unsafe extern "C" fn exported_handle(
    _window: *mut gdk_wayland_sys::GdkWaylandWindow,
    handle: *const c_char,
    data: *mut c_void,
) {
    // SAFETY: data is our boxed sender, owned by GDK until destroy_sender.
    // GDK's handle is borrowed for this callback only; retain an owned string.
    let tx = unsafe { &*data.cast::<Sender<String>>() };
    let identifier = if handle.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(handle) }
            .to_str()
            .ok()
            .filter(|handle| !handle.is_empty())
            .map_or_else(String::new, |handle| format!("wayland:{handle}"))
    };
    let _ = tx.try_send(identifier);
}

unsafe extern "C" fn destroy_sender(data: *mut c_void) {
    // SAFETY: this is the same allocation passed to GDK, reclaimed once.
    drop(unsafe { Box::from_raw(data.cast::<Sender<String>>()) });
}
