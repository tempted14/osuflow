//! The commands the webview can call, plus the shared app state they run against.

use crate::{
    downloader::{DownloadState, Downloader, EventSink, NameHint, ProgressEvent, StateEvent},
    error::{AppError, Result},
    handoff::Handoff,
    hotkey::{self, HotkeyStatus},
    importer::ImportMethod,
    library::Library,
    mirrors::{MirrorId, MirrorSet, SearchQuery, SearchResponse},
    models::Beatmapset,
    osu_locator::{self, OsuInstall},
    overlay::{self, OverlayState},
    settings::Settings,
    tray,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};
use tauri::{AppHandle, Emitter, State};

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub settings_path: PathBuf,
    pub mirrors: Arc<MirrorSet>,
    pub downloader: Arc<Downloader>,
    pub handoff: Handoff,
    pub library: Arc<Library>,
    pub overlay: OverlayState,
    pub hotkey_status: Mutex<HotkeyStatus>,
}

impl AppState {
    pub(crate) fn settings_snapshot(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }
}

/// Forwards download events to the webview as `download-state` / `download-progress`.
pub struct TauriSink(pub AppHandle);

impl EventSink for TauriSink {
    fn state(&self, event: StateEvent) {
        let _ = self.0.emit("download-state", event);
    }

    fn progress(&self, event: ProgressEvent) {
        let _ = self.0.emit("download-progress", event);
    }
}

#[tauri::command]
pub async fn search(state: State<'_, AppState>, query: SearchQuery) -> Result<SearchResponse> {
    let order = state.settings_snapshot().mirror_order;
    state.mirrors.search(&order, &query).await
}

/// Downloads a beatmapset and hands it to osu!. Progress arrives as events; the returned value
/// says how it was handed over. Success means the set's folder really appeared in Songs; if
/// osu! is busy or never imports it, the error says so and the download is kept.
#[tauri::command]
pub async fn download_beatmapset(
    state: State<'_, AppState>,
    set_id: u32,
    artist: Option<String>,
    title: Option<String>,
) -> Result<ImportMethod> {
    download_and_import(&state, set_id, NameHint::from_parts(artist, title)).await
}

/// The whole pipeline - download, guarded hand-off, progress events - shared by the overlay and
/// the link handler. `hint` is the artist and title when the caller already has them, which saves
/// asking a mirror for them to name the file.
pub async fn download_and_import(state: &AppState, set_id: u32, hint: Option<NameHint>) -> Result<ImportMethod> {
    let settings = state.settings_snapshot();
    // Downloads go to the faster mirror first; search keeps the order in the settings.
    let order = MirrorId::for_downloads(&settings.mirror_order);
    let archive = state
        .downloader
        .fetch_named(set_id, settings.no_video, &order, hint.as_ref())
        .await?;
    state.downloader.emit_state(set_id, DownloadState::Importing, None);
    match state.handoff.hand_off(&archive, set_id, &settings).await {
        Ok(method) => {
            state.downloader.emit_state(set_id, DownloadState::Done, None);
            Ok(method)
        }
        Err(error) => {
            state
                .downloader
                .emit_state(set_id, DownloadState::Failed, Some(error.to_string()));
            Err(error)
        }
    }
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, set_id: u32) -> bool {
    state.downloader.cancel(set_id)
}

/// Beatmap id (from a `/b/{id}` link) to the id of the set that contains it.
#[tauri::command]
pub async fn resolve_set_id(state: State<'_, AppState>, beatmap_id: u32) -> Result<u32> {
    let order = state.settings_snapshot().mirror_order;
    state.mirrors.resolve_set_id(&order, beatmap_id).await
}

#[tauri::command]
pub async fn lookup_beatmapset(state: State<'_, AppState>, set_id: u32) -> Result<Beatmapset> {
    let order = state.settings_snapshot().mirror_order;
    state.mirrors.lookup_set(&order, set_id).await
}

#[tauri::command]
pub async fn detect_osu(state: State<'_, AppState>) -> Result<Option<OsuInstall>> {
    let settings = state.settings_snapshot();
    tauri::async_runtime::spawn_blocking(move || osu_locator::detect(&settings))
        .await
        .map_err(|e| AppError::Task(e.to_string()))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings_snapshot()
}

/// Saves everything except the hotkey, which only `set_hotkey` may change because it also has
/// to re-register the shortcut with Windows.
#[tauri::command]
pub fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    let mut settings = settings.normalized();
    settings.hotkey = state.settings_snapshot().hotkey;
    settings.save(&state.settings_path)?;
    *state.settings.write().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", &settings);
    Ok(settings)
}

/// Ids of every beatmapset already in the Songs folder. Also (re)starts the folder watcher
/// that produces `library-changed` events.
#[tauri::command]
pub async fn get_installed(state: State<'_, AppState>) -> Result<Vec<u32>> {
    let settings = state.settings_snapshot();
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let songs = osu_locator::detect(&settings)
            .map(|install| install.songs_dir)
            .or_else(|| settings.songs_dir.clone());
        library.sync(songs.as_deref())
    })
    .await
    .map_err(|e| AppError::Task(e.to_string()))
}

#[tauri::command]
pub async fn show_overlay(app: AppHandle) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || overlay::show(&app))
        .await
        .map_err(|e| AppError::Task(e.to_string()))
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) {
    overlay::hide(&app, true);
}

#[tauri::command]
pub async fn toggle_overlay(app: AppHandle) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || overlay::toggle(&app))
        .await
        .map_err(|e| AppError::Task(e.to_string()))
}

#[tauri::command]
pub fn get_hotkey_status(state: State<'_, AppState>) -> HotkeyStatus {
    state.hotkey_status.lock().unwrap().clone()
}

/// Changes the global hotkey. If Windows refuses the new one (another program owns it) the old
/// one is put back, so the overlay never ends up without a shortcut.
#[tauri::command]
pub fn set_hotkey(app: AppHandle, state: State<'_, AppState>, hotkey: String) -> Result<HotkeyStatus> {
    let hotkey = hotkey.trim().to_string();
    hotkey::parse(&hotkey)?;
    let previous = state.settings_snapshot().hotkey;
    {
        let status = state.hotkey_status.lock().unwrap();
        if hotkey == previous && status.registered {
            return Ok(status.clone());
        }
    }

    hotkey::unregister(&app, &previous);
    if let Err(error) = hotkey::register(&app, &hotkey) {
        let restored = hotkey::register(&app, &previous);
        *state.hotkey_status.lock().unwrap() = HotkeyStatus::from_result(&previous, &restored);
        return Err(error);
    }

    let mut settings = state.settings_snapshot();
    settings.hotkey = hotkey.clone();
    settings.save(&state.settings_path)?;
    *state.settings.write().unwrap() = settings.clone();
    let status = HotkeyStatus::from_result(&hotkey, &Ok(()));
    *state.hotkey_status.lock().unwrap() = status.clone();
    tray::refresh_hotkey(&app, &hotkey);
    let _ = app.emit("settings-changed", &settings);
    Ok(status)
}

/// Whether osu!flow starts quietly in the tray when Windows starts.
#[tauri::command]
pub fn get_autostart() -> bool {
    tray::autostart_enabled()
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool> {
    Ok(tray::set_autostart(&app, enabled)?)
}

/// Opens a beatmapset's page on osu.ppy.sh in the default browser.
#[tauri::command]
pub fn open_beatmapset_page(app: AppHandle, set_id: u32) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(format!("https://osu.ppy.sh/beatmapsets/{set_id}"), None::<&str>)
        .map_err(|e| AppError::Task(e.to_string()))
}
