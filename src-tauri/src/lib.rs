mod autostart;
mod browser;
mod commands;
mod downloader;
mod error;
mod glass;
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
mod ui;

use std::{
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, so a second launch (Windows opening a link with osu!flow already running) is
        // caught and handed to the running app before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            match link_url::first_link_in_args(argv.into_iter().skip(1)) {
                Some(url) => links::handle_url(app, url),
                None => ui::show_settings(app),
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            // Settings kept under the app's earlier name (QuickDirect) move across once.
            settings::migrate_legacy_settings(&settings_path);
            let first_run = !settings_path.exists();
            let settings = settings::Settings::load(&settings_path);
            if first_run {
                // Written straight away, so only the very first start counts as a first run.
                let _ = settings.save(&settings_path);
            }
            let hotkey = settings.hotkey.clone();
            let link_handler = settings.link_handler;
            let args: Vec<String> = std::env::args().skip(1).collect();
            // Set when Windows started osu!flow to open a link.
            let startup_link = link_url::first_link_in_args(args.iter().cloned());
            // Set by the "Start with Windows" entry: sit in the tray without showing a window.
            let background = args.iter().any(|arg| arg == autostart::BACKGROUND_FLAG);

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

            // Started by a link with nothing for osu!flow to do with it (not a beatmap link, or
            // the handler is off): pass it on and leave without opening any window, so ordinary
            // links stay as quick as they were.
            if let Some(url) = &startup_link
                && (!link_handler || link_url::parse_link(url).is_none())
            {
                let _ = links::forward_url(app.handle(), url);
                std::process::exit(0);
            }

            // Windows are declared with `create: false` in tauri.conf.json and built here, after
            // the state exists. Otherwise a page that loads instantly (the packaged app) can call
            // a command before `manage` has run and get "state not managed" back.
            ui::create_startup_windows(app.handle(), link_handler)?;

            // A hotkey another program already owns must not stop the app: the settings window
            // shows the problem and lets the user pick another.
            let registered = hotkey::register(app.handle(), &hotkey);
            *app.state::<commands::AppState>().hotkey_status.lock().unwrap() =
                hotkey::HotkeyStatus::from_result(&hotkey, &registered);

            tray::build(app.handle(), &hotkey)?;
            if let Some(url) = startup_link {
                // Started by a beatmap link: only the toast shows.
                links::handle_url(app.handle(), url);
            } else if first_run || !background {
                // Started by hand (or for the first time): say hello. At login nothing shows.
                ui::show_settings(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Clicking or alt-tabbing away from the overlay dismisses it.
            if let (overlay::LABEL, WindowEvent::Focused(false)) = (window.label(), event) {
                let app = window.app_handle();
                if app.state::<commands::AppState>().overlay.focus_loss_is_real() {
                    overlay::hide(app, false);
                }
            }
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
            commands::get_autostart,
            commands::set_autostart,
            links::get_link_handler_status,
            links::set_link_handler,
            links::remove_link_registration,
            links::open_default_apps,
            links::toast_open_page,
            links::toast_retry,
            links::dismiss_toast,
            links::get_current_toast,
        ])
        .build(tauri::generate_context!())
        .expect("error while building osu!flow")
        .run(|_app, event| {
            // Closing the last window (the settings window is destroyed when closed) must not
            // quit: osu!flow lives in the tray until "Quit" is chosen.
            if let RunEvent::ExitRequested { api, code, .. } = event
                && code.is_none()
            {
                api.prevent_exit();
            }
        });
}
