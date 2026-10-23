//! The commands the webview can call, plus the shared app state they run against.

use crate::{
    downloader::{DownloadState, Downloader, EventSink, ProgressEvent, StateEvent},
    error::{AppError, Result},
    importer::{self, ImportMethod},
    mirrors::{MirrorSet, SearchQuery, SearchResponse},
    models::Beatmapset,
    osu_locator::{self, OsuInstall},
    settings::Settings,
};
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tauri::{AppHandle, Emitter, State};

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub settings_path: PathBuf,
    pub mirrors: Arc<MirrorSet>,
    pub downloader: Arc<Downloader>,
}

impl AppState {
    fn settings_snapshot(&self) -> Settings {
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
/// says how it was handed over.
#[tauri::command]
pub async fn download_beatmapset(state: State<'_, AppState>, set_id: u32) -> Result<ImportMethod> {
    let settings = state.settings_snapshot();
    let archive = state
        .downloader
        .fetch(set_id, settings.no_video, &settings.mirror_order)
        .await?;
    state.downloader.emit_state(set_id, DownloadState::Importing, None);
    match import_blocking(settings, archive).await {
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

/// Detecting osu! scans the process list, so it runs off the async runtime.
async fn import_blocking(settings: Settings, archive: PathBuf) -> Result<ImportMethod> {
    tauri::async_runtime::spawn_blocking(move || {
        let install = osu_locator::detect(&settings);
        let songs = install
            .as_ref()
            .map(|i| i.songs_dir.clone())
            .or_else(|| settings.songs_dir.clone());
        importer::import(&archive, install.as_ref(), songs.as_deref())
    })
    .await
    .map_err(|e| AppError::Task(e.to_string()))?
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

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    let settings = settings.normalized();
    settings.save(&state.settings_path)?;
    *state.settings.write().unwrap() = settings.clone();
    Ok(settings)
}
