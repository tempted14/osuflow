//! The opt-in link handler: what happens when Windows passes osu!flow a link because it is
//! the default browser.
//!
//! Beatmap links download the map through the same guarded pipeline the overlay uses and show a
//! small toast; every other link - and every link while the handler is switched off - goes
//! straight to the browser osu!flow replaced.

use crate::{
    browser::{self, Registry},
    commands::{self, AppState},
    downloader::NameHint,
    error::{AppError, Result},
    importer::ImportMethod,
    library,
    link_url::{self, LinkTarget},
    osu_locator,
    settings::Settings,
    ui,
};
use reqwest::Url;
use serde::Serialize;
use std::{collections::HashMap, path::Path, sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State};

pub const TOAST_LABEL: &str = "toast";
/// Looking up a map's name is only for the toast, so it never holds a download up long.
const LABEL_LOOKUP_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToastState {
    Resolving,
    Installed,
    Working,
    Done,
    Failed,
    Hidden,
}

/// What the corner toast shows. Download progress arrives separately as `download-progress`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkToast {
    pub state: ToastState,
    pub set_id: Option<u32>,
    pub label: String,
    pub message: Option<String>,
    /// The link that was clicked, so the toast can offer to open the page or try again.
    pub url: String,
}

impl LinkToast {
    fn new(state: ToastState, set_id: Option<u32>, label: &str, message: Option<&str>, url: &Url) -> Self {
        Self {
            state,
            set_id,
            label: label.to_string(),
            message: message.map(str::to_string),
            url: url.to_string(),
        }
    }

    fn hidden(url: &Url) -> Self {
        Self::new(ToastState::Hidden, None, "", None, url)
    }
}

/// Everything the flow needs from the app, so it can be tested with a fake.
pub(crate) trait LinkEnv {
    async fn resolve_set_id(&self, beatmap_id: u32) -> Result<u32>;
    async fn set_label(&self, set_id: u32) -> Option<String>;
    async fn is_installed(&self, set_id: u32) -> bool;
    /// The artist and title the download can name its file after, once `set_label` has found them.
    fn name_hint(&self, _set_id: u32) -> Option<NameHint> {
        None
    }
    async fn download(&self, set_id: u32, hint: Option<NameHint>) -> Result<ImportMethod>;
    fn forward(&self, url: &Url);
    fn toast(&self, toast: LinkToast);
}

fn done_message(method: ImportMethod) -> &'static str {
    match method {
        ImportMethod::LaunchedOsu => "Added to osu!",
        ImportMethod::MovedToSongs => "Saved to your Songs folder - osu! picks it up when it refreshes",
    }
}

/// Handles one clicked link. Anything that isn't a beatmap link, or any link while the handler
/// is off, is passed straight on.
pub(crate) async fn process_link<E: LinkEnv>(env: &E, enabled: bool, url: Url) {
    let target = match link_url::parse_link(&url) {
        Some(target) if enabled => target,
        _ => {
            env.forward(&url);
            return;
        }
    };

    env.toast(LinkToast::new(ToastState::Resolving, None, "Beatmap link", Some("Looking up the map…"), &url));
    let set_id = match target {
        LinkTarget::Set(id) => id,
        LinkTarget::Beatmap(id) => match env.resolve_set_id(id).await {
            Ok(set_id) => set_id,
            Err(_) => {
                // Without the set there is nothing to download; let the browser show the page.
                env.toast(LinkToast::hidden(&url));
                env.forward(&url);
                return;
            }
        },
    };

    let label = env.set_label(set_id).await.unwrap_or_else(|| format!("Beatmapset {set_id}"));
    if env.is_installed(set_id).await {
        env.toast(LinkToast::new(ToastState::Installed, Some(set_id), &label, Some("Already in your library"), &url));
        return;
    }

    env.toast(LinkToast::new(ToastState::Working, Some(set_id), &label, Some("Downloading…"), &url));
    let finished = match env.download(set_id, env.name_hint(set_id)).await {
        Ok(method) => LinkToast::new(ToastState::Done, Some(set_id), &label, Some(done_message(method)), &url),
        Err(error) => LinkToast::new(ToastState::Failed, Some(set_id), &label, Some(&error.to_string()), &url),
    };
    env.toast(finished);
}

/// The real thing: mirrors, the library, the guarded hand-off and the toast window.
/// The second field remembers the names `set_label` found, so the download can reuse them.
struct AppLinkEnv(AppHandle, Mutex<HashMap<u32, NameHint>>);

impl AppLinkEnv {
    fn new(app: AppHandle) -> Self {
        Self(app, Mutex::default())
    }
}

impl LinkEnv for AppLinkEnv {
    async fn resolve_set_id(&self, beatmap_id: u32) -> Result<u32> {
        let state = self.0.state::<AppState>();
        let order = state.settings_snapshot().mirror_order;
        state.mirrors.resolve_set_id(&order, beatmap_id).await
    }

    async fn set_label(&self, set_id: u32) -> Option<String> {
        let state = self.0.state::<AppState>();
        let order = state.settings_snapshot().mirror_order;
        let lookup = tokio::time::timeout(LABEL_LOOKUP_TIMEOUT, state.mirrors.lookup_set(&order, set_id)).await;
        let set = lookup.ok()?.ok()?;
        self.1
            .lock()
            .unwrap()
            .insert(set_id, NameHint { artist: set.artist.clone(), title: set.title.clone() });
        Some(format!("{} – {}", set.artist, set.title))
    }

    fn name_hint(&self, set_id: u32) -> Option<NameHint> {
        self.1.lock().unwrap().get(&set_id).cloned()
    }

    async fn is_installed(&self, set_id: u32) -> bool {
        let settings = self.0.state::<AppState>().settings_snapshot();
        tauri::async_runtime::spawn_blocking(move || {
            osu_locator::detect(&settings).is_some_and(|install| library::has_folder_for(&install.songs_dir, set_id))
        })
        .await
        .unwrap_or(false)
    }

    async fn download(&self, set_id: u32, hint: Option<NameHint>) -> Result<ImportMethod> {
        commands::download_and_import(&self.0.state::<AppState>(), set_id, hint).await
    }

    fn forward(&self, url: &Url) {
        if let Err(error) = forward_url(&self.0, url) {
            show_toast(
                &self.0,
                &LinkToast::new(ToastState::Failed, None, "Couldn't open the link", Some(&error.to_string()), url),
            );
        }
    }

    fn toast(&self, toast: LinkToast) {
        show_toast(&self.0, &toast);
    }
}

/// Runs the link flow in the background.
pub fn handle_url(app: &AppHandle, url: Url) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let enabled = app.state::<AppState>().settings_snapshot().link_handler;
        process_link(&AppLinkEnv::new(app.clone()), enabled, url).await;
    });
}

/// Opens `url` in the browser osu!flow replaced (or a well-known fallback).
pub fn forward_url(app: &AppHandle, url: &Url) -> Result<()> {
    let saved = app.state::<AppState>().settings_snapshot().previous_browser;
    let me = std::env::current_exe()?;
    let browser = browser::choose_browser(saved.as_deref(), &browser::fallback_browsers(), &me, |p| p.is_file())
        .ok_or_else(|| AppError::Task("couldn't find a browser to open this link".into()))?;
    browser::open_in_browser(&browser, url)?;
    Ok(())
}

/// The toast on screen right now, so a toast window that was only just built (its page may not be
/// listening yet) can ask for it instead of missing the event.
static CURRENT_TOAST: Mutex<Option<LinkToast>> = Mutex::new(None);

#[tauri::command]
pub fn get_current_toast() -> Option<LinkToast> {
    CURRENT_TOAST.lock().unwrap().clone()
}

fn show_toast(app: &AppHandle, toast: &LinkToast) {
    if toast.state == ToastState::Hidden {
        *CURRENT_TOAST.lock().unwrap() = None;
        if let Some(window) = app.get_webview_window(TOAST_LABEL) {
            let _ = window.hide();
        }
        return;
    }
    *CURRENT_TOAST.lock().unwrap() = Some(toast.clone());
    // Built on first use when link handling was switched on after start-up.
    let Some(window) = ui::ensure(app, TOAST_LABEL) else { return };
    let _ = app.emit_to(TOAST_LABEL, "link-toast", toast);
    place_toast(app, &window);
    // Never `set_focus`: the toast also isn't focusable (tauri.conf.json), so appearing over a
    // game must not pull keyboard focus away from it.
    let _ = window.show();
}

/// Bottom-right corner of the primary monitor's usable area.
fn place_toast(app: &AppHandle, window: &tauri::WebviewWindow) {
    let (Some(monitor), Ok(size)) = (app.primary_monitor().ok().flatten(), window.outer_size()) else { return };
    let area = monitor.work_area();
    let margin = (16.0 * monitor.scale_factor()) as i32;
    let x = area.position.x + area.size.width as i32 - size.width as i32 - margin;
    let y = area.position.y + area.size.height as i32 - size.height as i32 - margin;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

// ---- settings screen --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkHandlerStatus {
    /// The user's switch.
    pub enabled: bool,
    /// osu!flow is listed as a browser in Windows' Default apps.
    pub registered: bool,
    /// The user has actually chosen it there - the step Windows won't let an app do for them.
    pub is_default: bool,
    pub previous_browser: Option<String>,
}

pub fn status_for(settings: &Settings, registry: &Registry, me: &Path) -> LinkHandlerStatus {
    LinkHandlerStatus {
        enabled: settings.link_handler,
        registered: registry.is_registered(),
        is_default: registry.is_default(me),
        previous_browser: settings.previous_browser.as_ref().map(|p| p.display().to_string()),
    }
}

/// Switches the handler on or off. Switching on registers osu!flow as a browser and
/// remembers the browser it will replace; switching off keeps the registration (and the
/// remembered browser) so links keep working until the user picks their browser again.
pub fn apply_link_handler(
    registry: &Registry,
    mut settings: Settings,
    enabled: bool,
    exe: &Path,
) -> std::io::Result<Settings> {
    if enabled {
        // None when osu!flow already is the default (switching back on): keep what we had.
        let replaced = registry.default_browser(exe);
        registry.register(exe)?;
        if replaced.is_some() {
            settings.previous_browser = replaced;
        }
    }
    settings.link_handler = enabled;
    Ok(settings)
}

/// Removes the browser registration, but only once it is no longer the default - removing it
/// while it is would leave Windows with nothing to open links with.
pub fn remove_registration(registry: &Registry, mut settings: Settings, me: &Path) -> Result<Settings> {
    if registry.is_default(me) {
        return Err(AppError::Task(
            "osu!flow is still your default browser. Choose another browser in Default apps first, otherwise links will stop opening."
                .into(),
        ));
    }
    registry.unregister()?;
    settings.link_handler = false;
    Ok(settings)
}

fn commit_settings(app: &AppHandle, state: &AppState, settings: Settings) -> Result<()> {
    settings.save(&state.settings_path)?;
    *state.settings.write().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

#[tauri::command]
pub fn get_link_handler_status(state: State<'_, AppState>) -> Result<LinkHandlerStatus> {
    Ok(status_for(&state.settings_snapshot(), &Registry::system(), &std::env::current_exe()?))
}

#[tauri::command]
pub fn set_link_handler(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<LinkHandlerStatus> {
    let registry = Registry::system();
    let exe = std::env::current_exe()?;
    let settings = apply_link_handler(&registry, state.settings_snapshot(), enabled, &exe)?;
    commit_settings(&app, &state, settings)?;
    // The toast's web view only exists while link handling is on.
    ui::sync_toast_window(&app, enabled);
    Ok(status_for(&state.settings_snapshot(), &registry, &exe))
}

#[tauri::command]
pub fn remove_link_registration(app: AppHandle, state: State<'_, AppState>) -> Result<LinkHandlerStatus> {
    let registry = Registry::system();
    let exe = std::env::current_exe()?;
    let settings = remove_registration(&registry, state.settings_snapshot(), &exe)?;
    commit_settings(&app, &state, settings)?;
    ui::sync_toast_window(&app, false);
    Ok(status_for(&state.settings_snapshot(), &registry, &exe))
}

/// Opens Windows' Default apps, straight to osu!flow's entry where the version allows.
#[tauri::command]
pub fn open_default_apps(app: AppHandle) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(format!("ms-settings:defaultapps?registeredAppUser={}", browser::APP_NAME), None::<&str>)
        .map_err(|e| AppError::Task(e.to_string()))
}

fn parse_web_url(text: &str) -> Result<Url> {
    Url::parse(text)
        .ok()
        .filter(link_url::is_web_url)
        .ok_or_else(|| AppError::Task("that isn't an http or https link".into()))
}

/// The toast's "Open page" button: the clicked link, in the user's usual browser.
#[tauri::command]
pub fn toast_open_page(app: AppHandle, url: String) -> Result<()> {
    forward_url(&app, &parse_web_url(&url)?)
}

/// The toast's "Try again" button.
#[tauri::command]
pub fn toast_retry(app: AppHandle, url: String) -> Result<()> {
    handle_url(&app, parse_web_url(&url)?);
    Ok(())
}

#[tauri::command]
pub fn dismiss_toast(app: AppHandle) {
    if let Some(window) = app.get_webview_window(TOAST_LABEL) {
        let _ = window.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records everything the flow does, in order.
    struct Fake {
        log: Mutex<Vec<String>>,
        resolves_to: Option<u32>,
        label: Option<&'static str>,
        installed: bool,
        download_error: Option<&'static str>,
        method: ImportMethod,
        hint: Option<NameHint>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                log: Mutex::default(),
                resolves_to: Some(39804),
                label: Some("xi – FREEDOM DiVE"),
                installed: false,
                download_error: None,
                method: ImportMethod::LaunchedOsu,
                hint: None,
            }
        }

        fn log(&self, entry: String) {
            self.log.lock().unwrap().push(entry);
        }

        fn entries(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    impl LinkEnv for Fake {
        async fn resolve_set_id(&self, beatmap_id: u32) -> Result<u32> {
            self.log(format!("resolve {beatmap_id}"));
            self.resolves_to.ok_or_else(|| AppError::Task("no mirror answered".into()))
        }

        async fn set_label(&self, _set_id: u32) -> Option<String> {
            self.label.map(str::to_string)
        }

        async fn is_installed(&self, _set_id: u32) -> bool {
            self.installed
        }

        fn name_hint(&self, _set_id: u32) -> Option<NameHint> {
            self.hint.clone()
        }

        async fn download(&self, set_id: u32, hint: Option<NameHint>) -> Result<ImportMethod> {
            match hint {
                Some(hint) => self.log(format!("download {set_id} named {} - {}", hint.artist, hint.title)),
                None => self.log(format!("download {set_id}")),
            }
            match self.download_error {
                Some(message) => Err(AppError::Task(message.into())),
                None => Ok(self.method),
            }
        }

        fn forward(&self, url: &Url) {
            self.log(format!("forward {url}"));
        }

        fn toast(&self, toast: LinkToast) {
            self.log(format!("toast {:?} [{}] {}", toast.state, toast.label, toast.message.unwrap_or_default()));
        }
    }

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[tokio::test]
    async fn ordinary_links_go_straight_to_the_browser() {
        let env = Fake::new();
        process_link(&env, true, url("https://example.com/page")).await;
        assert_eq!(env.entries(), ["forward https://example.com/page"]);
    }

    #[tokio::test]
    async fn pages_about_a_map_that_are_not_the_map_page_go_to_the_browser_too() {
        let env = Fake::new();
        process_link(&env, true, url("https://osu.ppy.sh/beatmapsets/39804/discussion")).await;
        assert_eq!(env.entries(), ["forward https://osu.ppy.sh/beatmapsets/39804/discussion"]);
    }

    #[tokio::test]
    async fn with_the_handler_off_even_beatmap_links_go_to_the_browser() {
        let env = Fake::new();
        process_link(&env, false, url("https://osu.ppy.sh/b/129891")).await;
        assert_eq!(env.entries(), ["forward https://osu.ppy.sh/b/129891"]);
    }

    #[tokio::test]
    async fn a_set_link_downloads_with_a_toast_at_each_stage() {
        let env = Fake::new();
        process_link(&env, true, url("https://osu.ppy.sh/beatmapsets/39804#osu/129891")).await;
        assert_eq!(
            env.entries(),
            [
                "toast Resolving [Beatmap link] Looking up the map…",
                "toast Working [xi – FREEDOM DiVE] Downloading…",
                "download 39804",
                "toast Done [xi – FREEDOM DiVE] Added to osu!",
            ]
        );
    }

    #[tokio::test]
    async fn the_name_found_for_the_toast_is_handed_to_the_download() {
        let env = Fake { hint: Some(NameHint { artist: "xi".into(), title: "FREEDOM DiVE".into() }), ..Fake::new() };
        process_link(&env, true, url("https://osu.ppy.sh/beatmapsets/39804")).await;
        assert!(env.entries().contains(&"download 39804 named xi - FREEDOM DiVE".to_string()), "{:?}", env.entries());
    }

    #[tokio::test]
    async fn a_beatmap_link_is_resolved_to_its_set_first() {
        let env = Fake::new();
        process_link(&env, true, url("https://osu.ppy.sh/b/129891")).await;
        let entries = env.entries();
        assert_eq!(entries[1], "resolve 129891");
        assert!(entries.contains(&"download 39804".to_string()), "{entries:?}");
    }

    #[tokio::test]
    async fn if_the_set_cannot_be_found_the_page_opens_in_the_browser_instead() {
        let env = Fake { resolves_to: None, ..Fake::new() };
        process_link(&env, true, url("https://osu.ppy.sh/b/5")).await;
        assert_eq!(
            env.entries(),
            [
                "toast Resolving [Beatmap link] Looking up the map…",
                "resolve 5",
                "toast Hidden [] ",
                "forward https://osu.ppy.sh/b/5",
            ]
        );
    }

    #[tokio::test]
    async fn a_map_already_in_the_library_is_not_downloaded_again() {
        let env = Fake { installed: true, ..Fake::new() };
        process_link(&env, true, url("https://osu.ppy.sh/s/39804")).await;
        let entries = env.entries();
        assert_eq!(entries.last().unwrap(), "toast Installed [xi – FREEDOM DiVE] Already in your library");
        assert!(!entries.iter().any(|e| e.starts_with("download")), "{entries:?}");
    }

    #[tokio::test]
    async fn a_failed_download_says_why() {
        let env = Fake { download_error: Some("osu! is showing a message or dialog"), ..Fake::new() };
        process_link(&env, true, url("https://osu.ppy.sh/s/39804")).await;
        assert_eq!(
            env.entries().last().unwrap(),
            "toast Failed [xi – FREEDOM DiVE] osu! is showing a message or dialog"
        );
    }

    #[tokio::test]
    async fn the_toast_still_names_the_map_when_the_lookup_fails() {
        let env = Fake { label: None, ..Fake::new() };
        process_link(&env, true, url("https://osu.ppy.sh/s/7")).await;
        assert!(env.entries().contains(&"toast Working [Beatmapset 7] Downloading…".to_string()));
    }

    #[tokio::test]
    async fn the_toast_explains_when_the_map_went_to_the_songs_folder() {
        let env = Fake { method: ImportMethod::MovedToSongs, ..Fake::new() };
        process_link(&env, true, url("https://osu.ppy.sh/s/7")).await;
        assert!(env.entries().last().unwrap().contains("Songs folder"));
    }

    #[test]
    fn toasts_serialise_for_the_page() {
        let toast = LinkToast::new(ToastState::Working, Some(5), "A – B", Some("Downloading…"), &url("https://osu.ppy.sh/s/5"));
        assert_eq!(
            serde_json::to_value(&toast).unwrap(),
            serde_json::json!({
                "state": "working", "setId": 5, "label": "A – B",
                "message": "Downloading…", "url": "https://osu.ppy.sh/s/5"
            })
        );
    }

    #[test]
    fn only_web_links_are_accepted_from_the_toast() {
        assert!(parse_web_url("https://osu.ppy.sh/b/1").is_ok());
        for bad in ["file:///C:/Windows/notepad.exe", "--flag", "javascript:alert(1)", ""] {
            assert!(parse_web_url(bad).is_err(), "{bad:?}");
        }
    }

    #[cfg(windows)]
    mod switch {
        use super::super::*;
        use crate::browser::sandbox;
        use std::path::PathBuf;

        const EXE: &str = r"C:\Apps\osu!flow\osuflow.exe";
        const CHROME: &str = r"C:\Browsers\chrome.exe";

        #[test]
        fn switching_on_registers_and_remembers_the_browser_being_replaced() {
            let (registry, _guard) = sandbox();
            registry.pretend_default(Some(CHROME));
            let settings = apply_link_handler(&registry, Settings::default(), true, Path::new(EXE)).unwrap();
            assert!(settings.link_handler);
            assert!(registry.is_registered());
            assert_eq!(settings.previous_browser, Some(PathBuf::from(r"C:\Browsers\chrome.exe")));
        }

        #[test]
        fn switching_back_on_while_osuflow_is_the_default_keeps_the_remembered_browser() {
            let (registry, _guard) = sandbox();
            registry.pretend_default(Some(CHROME));
            let first = apply_link_handler(&registry, Settings::default(), true, Path::new(EXE)).unwrap();
            registry.pretend_default(Some(EXE)); // the user has now chosen osu!flow
            let off = apply_link_handler(&registry, first, false, Path::new(EXE)).unwrap();
            let again = apply_link_handler(&registry, off, true, Path::new(EXE)).unwrap();
            assert_eq!(again.previous_browser, Some(PathBuf::from(r"C:\Browsers\chrome.exe")));
        }

        #[test]
        fn switching_off_keeps_the_registration_and_the_remembered_browser() {
            let (registry, _guard) = sandbox();
            let before = Settings { previous_browser: Some(PathBuf::from(r"C:\B\chrome.exe")), ..Settings::default() };
            let on = apply_link_handler(&registry, before, true, Path::new(EXE)).unwrap();
            let off = apply_link_handler(&registry, on, false, Path::new(EXE)).unwrap();
            assert!(!off.link_handler);
            assert!(registry.is_registered(), "links must keep working until the user picks a browser again");
            assert_eq!(off.previous_browser, Some(PathBuf::from(r"C:\B\chrome.exe")));
        }

        #[test]
        fn the_registration_can_be_removed_only_once_it_is_not_the_default() {
            let (registry, _guard) = sandbox();
            let on = apply_link_handler(&registry, Settings::default(), true, Path::new(EXE)).unwrap();

            registry.pretend_default(Some(EXE));
            let error = remove_registration(&registry, on.clone(), Path::new(EXE)).unwrap_err();
            assert!(error.to_string().contains("still your default browser"), "{error}");
            assert!(registry.is_registered(), "must not unregister while links still depend on it");

            registry.pretend_default(Some(CHROME)); // the user switched away
            let removed = remove_registration(&registry, on, Path::new(EXE)).unwrap();
            assert!(!removed.link_handler);
            assert!(!registry.is_registered());
        }

        #[test]
        fn the_status_reflects_the_registry_and_the_settings() {
            let (registry, _guard) = sandbox();
            let off = status_for(&Settings::default(), &registry, Path::new(EXE));
            assert!(!off.enabled && !off.registered && !off.is_default && off.previous_browser.is_none());

            let settings = apply_link_handler(&registry, Settings::default(), true, Path::new(EXE)).unwrap();
            let on = status_for(&settings, &registry, Path::new(EXE));
            assert!(on.enabled && on.registered && !on.is_default);

            registry.pretend_default(Some(EXE));
            assert!(status_for(&settings, &registry, Path::new(EXE)).is_default);
        }
    }
}
