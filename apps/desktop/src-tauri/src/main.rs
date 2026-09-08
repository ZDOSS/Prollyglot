fn main() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("DISPLAY").is_some() {
        // The initial Ubuntu caption window uses X11/XWayland positioning.
        // Native Wayland overlay placement remains a separate acceptance task.
        // SAFETY: this is the first operation in main, before GTK, Tauri, or any
        // application threads start. Respect an explicitly selected backend.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }
    prollyglot_desktop_lib::run();
}
