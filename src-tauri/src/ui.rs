//! The app's windows. The overlay lives for the whole session so the hotkey answers at once. The
//! settings window is built when it is wanted and destroyed when it is closed, and the link toast
//! only exists while link handling is on: a web view costs tens of megabytes, so a window that
//! isn't on screen shouldn't keep one.

use crate::{glass, links, overlay};
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};

pub const MAIN: &str = "main";

/// Builds the window declared with this label in tauri.conf.json (all are `create: false`, so
/// nothing exists until this runs) and gives it the glass backdrop.
fn build(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == label)
        .ok_or(tauri::Error::WindowNotFound)?;
    let window = WebviewWindowBuilder::from_config(app, config)?
        .initialization_script(glass::init_script())
        .build()?;
    glass::apply(&window);
    Ok(window)
}

/// The window with this label, built first if it doesn't exist. Building waits for the main
/// thread, so call this from a worker thread, never from an event handler that runs there.
pub fn ensure(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    app.get_webview_window(label).or_else(|| build(app, label).ok())
}

/// The windows every start needs: the overlay (hidden) and, while link handling is on, the toast.
pub fn create_startup_windows(app: &AppHandle, link_handler: bool) -> tauri::Result<()> {
    build(app, overlay::LABEL)?;
    if link_handler {
        build(app, links::TOAST_LABEL)?;
    }
    Ok(())
}

/// Shows the settings window, building it if it was closed. Safe to call from any thread.
pub fn show_settings(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(window) = ensure(&app, MAIN) {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    });
}

/// Builds the toast window when link handling is switched on and destroys it when switched off.
pub fn sync_toast_window(app: &AppHandle, link_handler: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if link_handler {
            let _ = ensure(&app, links::TOAST_LABEL);
        } else if let Some(window) = app.get_webview_window(links::TOAST_LABEL) {
            let _ = window.destroy();
        }
    });
}
