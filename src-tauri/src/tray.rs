//! The tray icon keeps the app reachable while no window is open: left click toggles the
//! overlay, the menu opens the settings window or quits.

use crate::overlay;
use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const TRAY_ID: &str = "main";

pub fn show_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn toggle_overlay_in_background(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || overlay::toggle(&app));
}

fn menu(app: &AppHandle, hotkey: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(app, "open", format!("Search maps ({hotkey})"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit QuickDirect", true, None::<&str>)?;
    Menu::with_items(app, &[&open, &settings, &separator, &quit])
}

pub fn build(app: &AppHandle, hotkey: &str) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("QuickDirect")
        .menu(&menu(app, hotkey)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => toggle_overlay_in_background(app),
            "settings" => show_settings_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_overlay_in_background(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Updates the menu text after the hotkey changes.
pub fn refresh_hotkey(app: &AppHandle, hotkey: &str) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), menu(app, hotkey)) {
        let _ = tray.set_menu(Some(menu));
    }
}
