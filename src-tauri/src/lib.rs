mod commands;
mod downloader;
mod error;
mod importer;
#[cfg(test)]
mod live_tests;
mod mirrors;
mod models;
mod osu_locator;
mod settings;

use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::Settings::load(&settings_path);

            let downloads_dir = app.path().app_local_data_dir()?.join("downloads");
            downloader::cleanup_old(&downloads_dir, Duration::from_secs(24 * 3600));

            let mirrors = Arc::new(mirrors::MirrorSet::new());
            let sink = Arc::new(commands::TauriSink(app.handle().clone()));
            let downloader = Arc::new(downloader::Downloader::new(mirrors.clone(), downloads_dir, sink));

            app.manage(commands::AppState {
                settings: RwLock::new(settings),
                settings_path,
                mirrors,
                downloader,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::download_beatmapset,
            commands::cancel_download,
            commands::resolve_set_id,
            commands::lookup_beatmapset,
            commands::detect_osu,
            commands::get_settings,
            commands::set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
