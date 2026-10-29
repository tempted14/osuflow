//! Only the overlay is an NSPanel, so it can join a fullscreen game's Space.
use tauri::{AppHandle, WebviewWindow};
use tauri_nspanel::{ManagerExt, WebviewWindowExt, cocoa::appkit::NSWindowCollectionBehavior};

/// Called once during setup, on AppKit's main thread, before the hidden overlay can be shown.
pub fn prepare(window: &WebviewWindow) -> tauri::Result<()> {
    let panel = window.to_panel()?;
    panel.set_style_mask(1 << 7); // NSWindowStyleMaskNonactivatingPanel, borderless.
    panel.set_level(25); // NSStatusWindowLevel: visible above fullscreen game windows.
    panel.set_collection_behaviour(NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary);
    panel.set_floating_panel(true);
    panel.set_hides_on_deactivate(false);
    panel.set_becomes_key_only_if_needed(false);
    Ok(())
}

pub fn show(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Ok(panel) = handle.get_webview_panel(crate::overlay::LABEL) { panel.show(); }
    });
}

pub fn hide(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Ok(panel) = handle.get_webview_panel(crate::overlay::LABEL) { panel.order_out(None); }
    });
}
