//! Windows 11 frosted glass. The blur is Windows' own acrylic system backdrop (the one menus and
//! flyouts use), applied behind a window together with rounded corners and a dark frame. DWM does
//! all the work, so it costs the app nothing: the page only paints translucent layers on top.
//! Older Windows has no such backdrop, and the page then falls back to solid colours
//! (`data-glass="off"`).

use std::sync::OnceLock;
use tauri::WebviewWindow;

/// DWM's system backdrop attribute arrived with Windows 11 22H2.
const MIN_BUILD: u32 = 22621;

fn build_supports_glass(build: Option<u32>) -> bool {
    build.is_some_and(|build| build >= MIN_BUILD)
}

/// Whether this Windows can draw the acrylic backdrop.
pub fn supported() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| build_supports_glass(windows_build()))
}

/// Runs first in every page, so the stylesheet knows which look to use before anything paints.
pub fn init_script() -> &'static str {
    if supported() {
        "document.documentElement.dataset.glass = 'on'"
    } else {
        "document.documentElement.dataset.glass = 'off'"
    }
}

#[cfg(windows)]
fn windows_build() -> Option<u32> {
    use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()?;
    let build: String = key.get_value("CurrentBuildNumber").ok()?;
    build.trim().parse().ok()
}

#[cfg(not(windows))]
fn windows_build() -> Option<u32> {
    None
}

/// Puts the acrylic backdrop, rounded corners and a dark frame on `window`. Returns whether
/// Windows accepted all of it; the window is still usable if it didn't.
#[cfg(windows)]
pub fn apply(window: &WebviewWindow) -> bool {
    use std::{ffi::c_void, mem::size_of};
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DWM_WINDOW_CORNER_PREFERENCE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
            DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
            DwmSetWindowAttribute,
        },
        UI::Controls::MARGINS,
    };

    if !supported() {
        return false;
    }
    let Ok(handle) = window.hwnd() else { return false };
    let hwnd = HWND(handle.0);

    let set = |attribute, value: i32| {
        // SAFETY: `value` outlives the call and its size is passed along with it.
        unsafe { DwmSetWindowAttribute(hwnd, attribute, &value as *const i32 as *const c_void, size_of::<i32>() as u32) }
            .is_ok()
    };
    // Extending the frame over the whole client area is what lets the backdrop show through.
    let margins = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
    // SAFETY: `margins` outlives the call.
    let extended = unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) }.is_ok();
    let corners: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    extended
        & set(DWMWA_USE_IMMERSIVE_DARK_MODE, 1)
        & set(DWMWA_WINDOW_CORNER_PREFERENCE, corners.0)
        & set(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW.0)
}

#[cfg(not(windows))]
pub fn apply(_window: &WebviewWindow) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_needs_windows_11_22h2_or_newer() {
        assert!(!build_supports_glass(None));
        assert!(!build_supports_glass(Some(19045))); // Windows 10 22H2
        assert!(!build_supports_glass(Some(22000))); // Windows 11 21H2
        assert!(build_supports_glass(Some(22621)));
        assert!(build_supports_glass(Some(26200)));
    }

    #[test]
    fn the_page_is_told_which_look_to_use() {
        let script = init_script();
        assert!(script.contains("dataset.glass"));
        assert!(script.contains(if supported() { "'on'" } else { "'off'" }));
    }
}
