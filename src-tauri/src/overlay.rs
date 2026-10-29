//! The overlay window: where it sits, when it shows, and where keyboard focus goes afterwards.
//! The window itself is declared in tauri.conf.json (hidden, borderless, transparent, always on
//! top) and gets its frosted-glass backdrop in `ui`; this module only moves, shows and hides it.
//! It is exactly the glass card, centred over osu!'s window, so the blur sits behind the card
//! alone and the game stays visible (and clickable) around it.

use crate::{commands::AppState, mirrors::MirrorId, osu_locator};
use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

pub const LABEL: &str = "overlay";
/// The card's size in logical pixels, shrunk to fit with a margin when the game window is small.
const PANEL_WIDTH: f64 = 1180.0;
const PANEL_HEIGHT: f64 = 860.0;
const PANEL_MARGIN: f64 = 48.0;
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

/// Where the glass card goes inside `bounds` (physical pixels, at display scale `scale`): its
/// usual size, or smaller to leave a margin around it, centred.
pub fn panel_rect(bounds: Rect, scale: f64) -> Rect {
    let fit = |wanted: f64, available: i32| {
        let margin = (PANEL_MARGIN * scale).round() as i32;
        ((wanted * scale).round() as i32).min((available - margin).max(1)).max(1)
    };
    let (width, height) = (fit(PANEL_WIDTH, bounds.width()), fit(PANEL_HEIGHT, bounds.height()));
    let left = bounds.left + (bounds.width() - width) / 2;
    let top = bounds.top + (bounds.height() - height) / 2;
    Rect { left, top, right: left + width, bottom: top + height }
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
    /// False on Unix: Wine processes are visible, native dialog/hung inspection is unavailable.
    pub window_inspection: bool,
    pub windows: Vec<WindowCandidate>,
}

pub fn snapshot_osu() -> OsuSnapshot {
    snapshot_client(crate::settings::GameClient::Stable)
}

fn snapshot_client(client: crate::settings::GameClient) -> OsuSnapshot {
    let pids: Vec<u32> = (if client == crate::settings::GameClient::Lazer { crate::lazer::running() } else { osu_locator::running_osu_processes() }).into_iter().map(|(pid, _)| pid).collect();
    snapshot_for_pids(pids)
}

/// The windows owned by `pids`. Split out so the dialog detection can be checked against a
/// real modal dialog in a throwaway process instead of osu!.
pub fn snapshot_for_pids(pids: Vec<u32>) -> OsuSnapshot {
    let windows = if pids.is_empty() { Vec::new() } else { win::windows_of(&pids) };
    OsuSnapshot { pids, windows, window_inspection: cfg!(windows) }
}

#[cfg(test)]
pub fn locate_osu_window() -> OsuWindow { locate_client_window(crate::settings::GameClient::Stable) }

fn locate_client_window(client: crate::settings::GameClient) -> OsuWindow {
    let snapshot = snapshot_client(client);
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

    let osu = locate_client_window(settings.game_client);
    let install = osu_locator::detect(&settings);
    let bounds = osu.window.map(|w| w.rect).or_else(|| primary_monitor_rect(app));
    if let Some(bounds) = bounds {
        // The monitor osu! is on decides the scale the card's logical size turns into pixels at.
        let centre = (f64::from(bounds.left + bounds.width() / 2), f64::from(bounds.top + bounds.height() / 2));
        let scale = app
            .monitor_from_point(centre.0, centre.1)
            .ok()
            .flatten()
            .map_or(1.0, |monitor| monitor.scale_factor());
        let rect = panel_rect(bounds, scale);
        let _ = window.set_position(PhysicalPosition::new(rect.left, rect.top));
        let _ = window.set_size(PhysicalSize::new(
            u32::try_from(rect.width()).unwrap_or(1).max(1),
            u32::try_from(rect.height()).unwrap_or(1).max(1),
        ));
    }

    // Open the connection the first download will use while the user is still searching.
    let downloader = state.downloader.clone();
    let order = MirrorId::for_downloads(&settings.mirror_order);
    tauri::async_runtime::spawn(async move { downloader.warm_up(&order) });

    // Whoever had focus gets it back on hide; osu! if nothing else did.
    state
        .overlay
        .on_shown(win::foreground_window_not_ours().or(osu.window.map(|w| w.handle)));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
    }
    #[cfg(target_os = "macos")]
    crate::macos_panel::show(app);
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
    let Some(_window) = app.get_webview_window(LABEL) else { return };
    let _ = app.emit_to(LABEL, "overlay-hidden", ());
    #[cfg(not(target_os = "macos"))]
    let _ = _window.hide();
    #[cfg(target_os = "macos")]
    crate::macos_panel::hide(app);
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
    fn the_card_is_centred_in_the_game_window_at_its_usual_size() {
        let game = Rect { left: 100, top: 50, right: 2660, bottom: 1491 }; // 2560 x 1441
        let card = panel_rect(game, 1.0);
        assert_eq!((card.width(), card.height()), (1180, 860));
        // Within a pixel: an odd leftover can't be split evenly.
        assert!(((card.left - game.left) - (game.right - card.right)).abs() <= 1, "centred horizontally");
        assert!(((card.top - game.top) - (game.bottom - card.bottom)).abs() <= 1, "centred vertically");
    }

    #[test]
    fn the_card_shrinks_to_leave_a_margin_in_a_small_window() {
        let small = Rect { left: 0, top: 0, right: 1024, bottom: 768 };
        let card = panel_rect(small, 1.0);
        assert_eq!((card.width(), card.height()), (1024 - 48, 768 - 48));
        assert_eq!((card.left, card.top), (24, 24));
    }

    #[test]
    fn the_card_scales_with_the_display() {
        let screen = Rect { left: 0, top: 0, right: 3840, bottom: 2160 };
        let card = panel_rect(screen, 1.5);
        assert_eq!((card.width(), card.height()), (1770, 1290));
        // A degenerate window still gives a usable rectangle.
        let sliver = panel_rect(Rect { left: 0, top: 0, right: 10, bottom: 10 }, 1.0);
        assert!(sliver.width() >= 1 && sliver.height() >= 1);
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
