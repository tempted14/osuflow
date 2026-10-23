//! The tray icon keeps osu!flow reachable while no window is open: a left click toggles the
//! overlay, and the menu opens the settings window, switches "Start with Windows" or quits.

use crate::{autostart::Autostart, commands::AppState, overlay, ui};
use tauri::{
    AppHandle, Emitter, Manager,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const TRAY_ID: &str = "main";

fn toggle_overlay_in_background(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || overlay::toggle(&app));
}

/// Whether Windows starts osu!flow at login.
pub fn autostart_enabled() -> bool {
    std::env::current_exe().is_ok_and(|exe| Autostart::system().is_enabled(&exe))
}

/// Switches "Start with Windows" and tells the tray menu and any open settings window.
pub fn set_autostart(app: &AppHandle, enabled: bool) -> std::io::Result<bool> {
    let autostart = Autostart::system();
    if enabled {
        autostart.enable(&std::env::current_exe()?)?;
    } else {
        autostart.disable()?;
    }
    let now = autostart_enabled();
    refresh(app);
    let _ = app.emit("autostart-changed", now);
    Ok(now)
}

fn menu(app: &AppHandle, hotkey: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(app, "open", format!("Search maps ({hotkey})"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let startup = CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, autostart_enabled(), None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit osu!flow", true, None::<&str>)?;
    Menu::with_items(app, &[&open, &settings, &startup, &separator, &quit])
}

pub fn build(app: &AppHandle, hotkey: &str) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("osu!flow")
        .menu(&menu(app, hotkey)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => toggle_overlay_in_background(app),
            "settings" => ui::show_settings(app),
            "autostart" => {
                let _ = set_autostart(app, !autostart_enabled());
            }
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

/// Rebuilds the menu from the current hotkey and autostart state.
pub fn refresh(app: &AppHandle) {
    let hotkey = app.state::<AppState>().settings_snapshot().hotkey;
    refresh_hotkey(app, &hotkey);
}

/// Updates the menu text after the hotkey changes.
pub fn refresh_hotkey(app: &AppHandle, hotkey: &str) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), menu(app, hotkey)) {
        let _ = tray.set_menu(Some(menu));
    }
}
