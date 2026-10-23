//! The overlay window: which window it covers, when it shows, and where keyboard focus goes
//! afterwards. The window itself is declared in tauri.conf.json (hidden, borderless,
//! transparent, always on top); this module only moves, shows and hides it.

use crate::{commands::AppState, osu_locator};
use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

pub const LABEL: &str = "overlay";
/// Focus can flicker while the window is coming up, so a loss this soon after showing is ignored.
const FOCUS_GRACE: Duration = Duration::from_millis(700);

/// A rectangle in physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }

    pub fn height(self) -> i32 {
        self.bottom - self.top
    }

    pub fn area(self) -> i64 {
        i64::from(self.width().max(0)) * i64::from(self.height().max(0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowCandidate {
    pub handle: isize,
    pub rect: Rect,
    pub visible: bool,
    pub minimized: bool,
    pub owned: bool,
    /// False while a modal dialog has the window locked.
    pub enabled: bool,
    /// Windows' own "not responding" judgement.
    pub hung: bool,
}

/// osu! owns a few helper windows besides the game; the one to cover is the biggest visible,
/// unminimised, top-level one.
pub fn pick_main_window(candidates: &[WindowCandidate]) -> Option<WindowCandidate> {
    candidates
        .iter()
        .copied()
        .filter(|c| c.visible && !c.minimized && !c.owned && c.rect.area() > 0)
        .max_by_key(|c| c.rect.area())
}

pub struct OsuWindow {
    pub running: bool,
    pub window: Option<WindowCandidate>,
}

/// osu!'s running processes and the top-level windows they own, as seen from outside. Reading
/// this sends osu! nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OsuSnapshot {
    pub pids: Vec<u32>,
    pub windows: Vec<WindowCandidate>,
}

pub fn snapshot_osu() -> OsuSnapshot {
    let pids: Vec<u32> = osu_locator::running_osu_processes().into_iter().map(|(pid, _)| pid).collect();
    snapshot_for_pids(pids)
}

/// The windows owned by `pids`. Split out so the dialog detection can be checked against a
/// real modal dialog in a throwaway process instead of osu!.
pub fn snapshot_for_pids(pids: Vec<u32>) -> OsuSnapshot {
    let windows = if pids.is_empty() { Vec::new() } else { win::windows_of(&pids) };
    OsuSnapshot { pids, windows }
}

pub fn locate_osu_window() -> OsuWindow {
    let snapshot = snapshot_osu();
    OsuWindow { running: !snapshot.pids.is_empty(), window: pick_main_window(&snapshot.windows) }
}

/// What the overlay needs to know when it opens, sent with the `overlay-shown` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayContext {
    pub osu_running: bool,
    pub exclusive_fullscreen: bool,
}

#[derive(Default)]
struct Inner {
    return_to: Option<isize>,
    shown_at: Option<Instant>,
}

#[derive(Default)]
pub struct OverlayState {
    inner: Mutex<Inner>,
}

impl OverlayState {
    fn on_shown(&self, return_to: Option<isize>) {
        *self.inner.lock().unwrap() = Inner { return_to, shown_at: Some(Instant::now()) };
    }

    fn take_return_target(&self) -> Option<isize> {
        self.inner.lock().unwrap().return_to.take()
    }

    /// False during the first moments after showing, when focus is still settling.
    pub fn focus_loss_is_real(&self) -> bool {
        self.inner.lock().unwrap().shown_at.is_none_or(|at| at.elapsed() > FOCUS_GRACE)
    }
}

fn primary_monitor_rect(app: &AppHandle) -> Option<Rect> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let (position, size) = (monitor.position(), monitor.size());
    Some(Rect {
        left: position.x,
        top: position.y,
        right: position.x + i32::try_from(size.width).ok()?,
        bottom: position.y + i32::try_from(size.height).ok()?,
    })
}

/// Covers osu!'s window (or the primary monitor when osu! isn't running), takes keyboard focus
/// and tells the page it is visible.
pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap().clone();

    let osu = locate_osu_window();
    let install = osu_locator::detect(&settings);
    let bounds = osu.window.map(|w| w.rect).or_else(|| primary_monitor_rect(app));
    if let Some(rect) = bounds {
        let _ = window.set_position(PhysicalPosition::new(rect.left, rect.top));
        let _ = window.set_size(PhysicalSize::new(
            u32::try_from(rect.width()).unwrap_or(1).max(1),
            u32::try_from(rect.height()).unwrap_or(1).max(1),
        ));
    }

    // Whoever had focus gets it back on hide; osu! if nothing else did.
    state
        .overlay
        .on_shown(win::foreground_window_not_ours().or(osu.window.map(|w| w.handle)));
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    let _ = app.emit_to(
        LABEL,
        "overlay-shown",
        OverlayContext {
            osu_running: osu.running,
            exclusive_fullscreen: install.is_some_and(|i| i.exclusive_fullscreen),
        },
    );
}

/// Hides the overlay. `refocus` hands keyboard focus back to the window that had it before.
pub fn hide(app: &AppHandle, refocus: bool) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let _ = app.emit_to(LABEL, "overlay-hidden", ());
    let _ = window.hide();
    let target = app.state::<AppState>().overlay.take_return_target();
    if let Some(handle) = target.filter(|_| refocus) {
        win::focus(handle);
    }
}

pub fn toggle(app: &AppHandle) {
    let visible = app
        .get_webview_window(LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide(app, true);
    } else {
        show(app);
    }
}

#[cfg(windows)]
mod win {
    use super::{Rect, WindowCandidate};
    use std::ffi::c_void;
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, RECT},
            UI::{
                Input::KeyboardAndMouse::IsWindowEnabled,
                WindowsAndMessaging::{
                    EnumWindows, GW_OWNER, GetForegroundWindow, GetWindow, GetWindowRect,
                    GetWindowThreadProcessId, IsHungAppWindow, IsIconic, IsWindow, IsWindowVisible,
                    SetForegroundWindow,
                },
            },
        },
        core::BOOL,
    };

    struct Search {
        pids: Vec<u32>,
        found: Vec<WindowCandidate>,
    }

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `&mut Search` handed to EnumWindows in `windows_of`, which
        // outlives the enumeration.
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if search.pids.contains(&pid) {
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
                search.found.push(WindowCandidate {
                    handle: hwnd.0 as isize,
                    rect: Rect { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom },
                    visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
                    minimized: unsafe { IsIconic(hwnd) }.as_bool(),
                    owned: unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok_and(|owner| !owner.0.is_null()),
                    enabled: unsafe { IsWindowEnabled(hwnd) }.as_bool(),
                    hung: unsafe { IsHungAppWindow(hwnd) }.as_bool(),
                });
            }
        }
        BOOL(1)
    }

    /// Every top-level window belonging to one of `pids`.
    pub fn windows_of(pids: &[u32]) -> Vec<WindowCandidate> {
        let mut search = Search { pids: pids.to_vec(), found: Vec::new() };
        // SAFETY: the callback only touches `search`, which lives across the call.
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut search as *mut Search as isize));
        }
        search.found
    }

    fn hwnd(handle: isize) -> HWND {
        HWND(handle as *mut c_void)
    }

    /// The window with keyboard focus right now, unless it belongs to this app.
    pub fn foreground_window_not_ours() -> Option<isize> {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(foreground, Some(&mut pid)) };
        (pid != std::process::id()).then_some(foreground.0 as isize)
    }

    pub fn focus(handle: isize) {
        let window = hwnd(handle);
        if unsafe { IsWindow(Some(window)) }.as_bool() {
            let _ = unsafe { SetForegroundWindow(window) };
        }
    }
}

#[cfg(not(windows))]
mod win {
    use super::WindowCandidate;

    pub fn windows_of(_pids: &[u32]) -> Vec<WindowCandidate> {
        Vec::new()
    }

    pub fn foreground_window_not_ours() -> Option<isize> {
        None
    }

    pub fn focus(_handle: isize) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(handle: isize, width: i32, height: i32) -> WindowCandidate {
        WindowCandidate {
            handle,
            rect: Rect { left: 10, top: 20, right: 10 + width, bottom: 20 + height },
            visible: true,
            minimized: false,
            owned: false,
            enabled: true,
            hung: false,
        }
    }

    #[test]
    fn rect_measures_itself() {
        let rect = Rect { left: -1920, top: 0, right: 0, bottom: 1080 };
        assert_eq!((rect.width(), rect.height()), (1920, 1080));
        assert_eq!(rect.area(), 1920 * 1080);
        assert_eq!(Rect { left: 5, top: 5, right: 0, bottom: 0 }.area(), 0, "inverted rects have no area");
    }

    #[test]
    fn picks_the_biggest_usable_window() {
        let game = candidate(1, 2560, 1440);
        let helper = candidate(2, 300, 200);
        assert_eq!(pick_main_window(&[helper, game]), Some(game));
        assert_eq!(pick_main_window(&[game, helper]), Some(game));
    }

    #[test]
    fn skips_hidden_minimised_owned_and_empty_windows() {
        let usable = candidate(1, 800, 600);
        let hidden = WindowCandidate { visible: false, ..candidate(2, 3000, 2000) };
        let minimised = WindowCandidate { minimized: true, ..candidate(3, 3000, 2000) };
        let owned = WindowCandidate { owned: true, ..candidate(4, 3000, 2000) };
        let empty = candidate(5, 0, 0);
        assert_eq!(pick_main_window(&[hidden, minimised, owned, empty, usable]), Some(usable));
        assert_eq!(pick_main_window(&[hidden, minimised, owned, empty]), None);
        assert_eq!(pick_main_window(&[]), None);
    }

    #[test]
    fn focus_loss_is_ignored_right_after_showing() {
        let state = OverlayState::default();
        assert!(state.focus_loss_is_real(), "never shown: nothing to ignore");
        state.on_shown(Some(7));
        assert!(!state.focus_loss_is_real());
        assert_eq!(state.take_return_target(), Some(7));
        assert_eq!(state.take_return_target(), None, "the target is handed back once");
    }

    #[test]
    fn overlay_context_serialises_for_the_page() {
        let json = serde_json::to_value(OverlayContext { osu_running: true, exclusive_fullscreen: false }).unwrap();
        assert_eq!(json, serde_json::json!({ "osuRunning": true, "exclusiveFullscreen": false }));
    }
}
