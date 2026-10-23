//! The global hotkey that opens the overlay.

use crate::{
    error::{AppError, Result},
    overlay,
};
use serde::Serialize;
use std::str::FromStr;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const DEFAULT_HOTKEY: &str = "Ctrl+Shift+D";

/// What the settings screen shows: the configured hotkey and whether Windows accepted it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub hotkey: String,
    pub registered: bool,
    pub error: Option<String>,
}

impl HotkeyStatus {
    pub fn from_result(hotkey: &str, result: &Result<()>) -> Self {
        Self {
            hotkey: hotkey.to_string(),
            registered: result.is_ok(),
            error: result.as_ref().err().map(|e| e.to_string()),
        }
    }
}

/// Parses a hotkey like `Ctrl+Shift+D`. A system-wide shortcut needs a modifier (or must be an
/// F-key), otherwise a plain letter would stop working in every other program.
pub fn parse(hotkey: &str) -> Result<Shortcut> {
    let shortcut = Shortcut::from_str(hotkey.trim())
        .map_err(|e| AppError::Hotkey(format!("'{}' isn't a valid shortcut ({e})", hotkey.trim())))?;
    let key = format!("{:?}", shortcut.key);
    let is_function_key = key.strip_prefix('F').is_some_and(|n| n.parse::<u8>().is_ok());
    if shortcut.mods.is_empty() && !is_function_key {
        return Err(AppError::Hotkey(
            "add Ctrl, Alt or Shift - a plain key would stop working everywhere else".into(),
        ));
    }
    Ok(shortcut)
}

pub fn register(app: &AppHandle, hotkey: &str) -> Result<()> {
    let shortcut = parse(hotkey)?;
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let app = app.clone();
                // Finding osu!'s window scans processes, so keep it off the event loop.
                tauri::async_runtime::spawn_blocking(move || overlay::toggle(&app));
            }
        })
        .map_err(|e| {
            AppError::Hotkey(format!("couldn't register {hotkey} - is another program using it? ({e})"))
        })
}

pub fn unregister(app: &AppHandle, hotkey: &str) {
    if let Ok(shortcut) = Shortcut::from_str(hotkey.trim()) {
        let _ = app.global_shortcut().unregister(shortcut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_modifier_combos_and_function_keys() {
        for ok in ["Ctrl+Shift+D", "alt+space", "CommandOrControl+Shift+O", "F8", "Ctrl+Alt+F12", " Ctrl+K "] {
            assert!(parse(ok).is_ok(), "{ok} should be accepted");
        }
    }

    #[test]
    fn rejects_bare_keys_and_nonsense() {
        for bad in ["", "D", "Space", "Ctrl+", "Ctrl+Shift+Banana", "Ctrl+A+B", "Shift+Ctrl"] {
            assert!(parse(bad).is_err(), "{bad:?} should be rejected");
        }
        let message = parse("D").unwrap_err().to_string();
        assert!(message.contains("Ctrl, Alt or Shift"), "{message}");
    }

    #[test]
    fn the_default_hotkey_is_valid() {
        assert!(parse(DEFAULT_HOTKEY).is_ok());
    }

    #[test]
    fn status_reports_success_and_failure() {
        let ok = HotkeyStatus::from_result("Ctrl+K", &Ok(()));
        assert!(ok.registered && ok.error.is_none());
        let failed = HotkeyStatus::from_result("Ctrl+K", &Err(AppError::Hotkey("taken".into())));
        assert!(!failed.registered);
        assert!(failed.error.unwrap().contains("taken"));
    }
}
