mod browser;
mod commands;
mod downloader;
mod error;
mod handoff;
mod hotkey;
mod importer;
mod library;
mod link_url;
mod links;
#[cfg(test)]
mod live_tests;
mod mirrors;
mod models;
mod osu_locator;
mod overlay;
mod settings;
mod tray;

use std::{
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tauri::{Emitter, Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, so a second launch (Windows opening a link with QuickDirect already running) is
        // caught and handed to the running app before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            match link_url::first_link_in_args(argv.into_iter().skip(1)) {
                Some(url) => links::handle_url(app, url),
                None => tray::show_settings_window(app),
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::Settings::load(&settings_path);
            let hotkey = settings.hotkey.clone();
            let link_handler = settings.link_handler;
            // Set when Windows started QuickDirect to open a link.
            let startup_link = link_url::first_link_in_args(std::env::args().skip(1));

            let downloads_dir = app.path().app_local_data_dir()?.join("downloads");
            downloader::cleanup_old(&downloads_dir, Duration::from_secs(24 * 3600));

            let handle = app.handle().clone();
            let library = Arc::new(library::Library::new(Arc::new(move |delta| {
                let _ = handle.emit("library-changed", delta);
            })));

            let mirrors = Arc::new(mirrors::MirrorSet::new());
            let sink = Arc::new(commands::TauriSink(app.handle().clone()));
            let downloader = Arc::new(downloader::Downloader::new(mirrors.clone(), downloads_dir, sink));

            // The state must exist before the hotkey can fire: the overlay reads it.
            app.manage(commands::AppState {
                settings: RwLock::new(settings),
                settings_path,
                mirrors,
                downloader,
                handoff: handoff::Handoff::new(Arc::new(handoff::SystemEnv)),
                library,
                overlay: overlay::OverlayState::default(),
                hotkey_status: Mutex::new(hotkey::HotkeyStatus::from_result(&hotkey, &Ok(()))),
            });

            // Windows are declared with `create: false` in tauri.conf.json and built here, after
            // the state exists. Otherwise a page that loads instantly (the packaged app) can call
            // a command before `manage` has run and get "state not managed" back.
            // Started by a link with nothing for QuickDirect to do with it (not a beatmap link,
            // or the handler is off): pass it on and leave without opening any window, so
            // ordinary links stay as quick as they were.
            if let Some(url) = &startup_link
                && (!link_handler || link_url::parse_link(url).is_none())
            {
                let _ = links::forward_url(app.handle(), url);
                std::process::exit(0);
            }

            for config in &app.config().app.windows {
                let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), config)?;
                // A beatmap link started us: stay in the background and let only the toast show.
                if config.label == "main" && startup_link.is_some() {
                    builder = builder.visible(false);
                }
                builder.build()?;
            }

            // A hotkey another program already owns must not stop the app: the settings window
            // shows the problem and lets the user pick another.
            let registered = hotkey::register(app.handle(), &hotkey);
            *app.state::<commands::AppState>().hotkey_status.lock().unwrap() =
                hotkey::HotkeyStatus::from_result(&hotkey, &registered);

            tray::build(app.handle(), &hotkey)?;
            if let Some(url) = startup_link {
                links::handle_url(app.handle(), url);
            }
            Ok(())
        })
        .on_window_event(|window, event| match (window.label(), event) {
            // Closing the settings window leaves the app running in the tray.
            ("main", WindowEvent::CloseRequested { api, .. }) => {
                api.prevent_close();
                let _ = window.hide();
            }
            // Clicking or alt-tabbing away from the overlay dismisses it.
            (overlay::LABEL, WindowEvent::Focused(false)) => {
                let app = window.app_handle();
                if app.state::<commands::AppState>().overlay.focus_loss_is_real() {
                    overlay::hide(app, false);
                }
            }
            _ => {}
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
            commands::get_installed,
            commands::show_overlay,
            commands::hide_overlay,
            commands::toggle_overlay,
            commands::get_hotkey_status,
            commands::set_hotkey,
            commands::open_beatmapset_page,
            links::get_link_handler_status,
            links::set_link_handler,
            links::remove_link_registration,
            links::open_default_apps,
            links::toast_open_page,
            links::toast_retry,
            links::dismiss_toast,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
