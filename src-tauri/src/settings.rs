use crate::{hotkey::DEFAULT_HOTKEY, mirrors::MirrorId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GameClient { #[default] Stable, Lazer }

/// User settings, stored as JSON in the app config dir. Unknown or missing keys are fine, so
/// older and newer builds can share a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub game_client: GameClient,
    pub lazer_exe_path: Option<PathBuf>,
    pub lazer_data_dir: Option<PathBuf>,
    pub lazer_launch_command: Option<String>,
    /// Override for `osu!.exe`; `None` means auto-detect.
    pub osu_exe_path: Option<PathBuf>,
    /// Override for the Songs folder; `None` means read it from osu!'s own config.
    pub songs_dir: Option<PathBuf>,
    /// Host path to a Wine prefix; used only on Unix.
    pub wine_prefix: Option<PathBuf>,
    /// Optional argv-style launch command. {file} is substituted without a shell.
    pub osu_launch_command: Option<String>,
    pub no_video: bool,
    /// Preferred mirror order; always contains every known mirror once after `normalized`.
    pub mirror_order: Vec<MirrorId>,
    /// Global shortcut that opens the overlay, e.g. `Ctrl+Shift+D`.
    pub hotkey: String,
    /// Leave results out of the overlay when they are already in the Songs folder.
    pub hide_installed: bool,
    /// Download beatmap links that Windows passes to osu!flow (when it is the default
    /// browser) instead of opening them. Off by default.
    pub link_handler: bool,
    /// The browser the link handler replaced, so every other link can still be opened in it.
    pub previous_browser: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            game_client: GameClient::Stable,
            lazer_exe_path: None,
            lazer_data_dir: None,
            lazer_launch_command: None,
            osu_exe_path: None,
            songs_dir: None,
            wine_prefix: None,
            osu_launch_command: None,
            no_video: true,
            mirror_order: MirrorId::DEFAULT_ORDER.to_vec(),
            hotkey: DEFAULT_HOTKEY.to_string(),
            hide_installed: false,
            link_handler: false,
            previous_browser: None,
        }
    }
}

/// The app was called QuickDirect before osu!flow, and Tauri keeps its files under the app's
/// identifier, so the old settings sit next to the new folder under this name.
const LEGACY_IDENTIFIER: &str = "com.quickdirect.app";

/// Where the settings file of the earlier QuickDirect build would be, given where the new one goes.
fn legacy_settings_path(path: &Path) -> Option<PathBuf> {
    // <config root>/<identifier>/settings.json
    Some(path.parent()?.parent()?.join(LEGACY_IDENTIFIER).join(path.file_name()?))
}

/// Copies the settings written under the app's earlier name to `path`, once: only when `path`
/// doesn't exist yet and the old file does. The old file is left where it is.
pub fn migrate_legacy_settings(path: &Path) {
    if path.exists() {
        return;
    }
    let Some(legacy) = legacy_settings_path(path).filter(|legacy| legacy.is_file()) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::copy(legacy, path);
}

impl Settings {
    /// Missing or unreadable files give defaults, so a corrupt file can never stop the app starting.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default()
            .normalized()
    }

    /// Writes to a temp file first so a crash can't leave a half-written settings file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)
    }

    pub fn normalized(mut self) -> Self {
        let mut order = Vec::new();
        for mirror in self.mirror_order.drain(..).chain(MirrorId::ALL) {
            if !order.contains(&mirror) {
                order.push(mirror);
            }
        }
        self.mirror_order = order;
        self.osu_exe_path = self.osu_exe_path.filter(|p| !p.as_os_str().is_empty());
        self.songs_dir = self.songs_dir.filter(|p| !p.as_os_str().is_empty());
        self.lazer_exe_path = self.lazer_exe_path.filter(|path| !path.as_os_str().is_empty());
        self.lazer_data_dir = self.lazer_data_dir.filter(|path| !path.as_os_str().is_empty());
        self.lazer_launch_command = self.lazer_launch_command.map(|command| command.trim().to_owned()).filter(|command| !command.is_empty());
        self.wine_prefix = self.wine_prefix.filter(|path| !path.as_os_str().is_empty());
        self.osu_launch_command = self.osu_launch_command.map(|command| command.trim().to_owned()).filter(|command| !command.is_empty());
        #[cfg(not(windows))]
        { self.link_handler = false; }
        self.hotkey = match self.hotkey.trim() {
            "" => DEFAULT_HOTKEY.to_string(),
            trimmed => trimmed.to_string(),
        };
        self.previous_browser = self.previous_browser.filter(|p| !p.as_os_str().is_empty());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_in(root: &Path, identifier: &str) -> PathBuf {
        root.join(identifier).join("settings.json")
    }

    #[test]
    fn settings_from_the_quickdirect_days_move_across_once() {
        let root = tempfile::tempdir().unwrap();
        let old = settings_in(root.path(), LEGACY_IDENTIFIER);
        let new = settings_in(root.path(), "com.osuflow.app");
        Settings { hotkey: "Ctrl+Alt+M".into(), hide_installed: true, ..Settings::default() }
            .save(&old)
            .unwrap();

        migrate_legacy_settings(&new);
        let migrated = Settings::load(&new);
        assert_eq!(migrated.hotkey, "Ctrl+Alt+M");
        assert!(migrated.hide_installed);
        assert!(old.is_file(), "the old file is left alone");

        // Later changes are never overwritten by the old file.
        Settings { hotkey: "Ctrl+Shift+F".into(), ..migrated }.save(&new).unwrap();
        migrate_legacy_settings(&new);
        assert_eq!(Settings::load(&new).hotkey, "Ctrl+Shift+F");
    }

    #[test]
    fn nothing_is_created_when_there_are_no_old_settings() {
        let root = tempfile::tempdir().unwrap();
        let new = settings_in(root.path(), "com.osuflow.app");
        migrate_legacy_settings(&new);
        assert!(!new.exists());
        assert!(!new.parent().unwrap().exists(), "no empty folder either");
    }

    #[test]
    fn defaults_prefer_no_video_and_list_every_mirror() {
        let s = Settings::default();
        assert!(s.no_video);
        assert_eq!(s.mirror_order, vec![MirrorId::OsuDirect, MirrorId::Catboy]);
    }

    #[test]
    fn overlay_defaults_are_ctrl_shift_d_and_show_installed_maps() {
        let s = Settings::default();
        assert_eq!(s.hotkey, DEFAULT_HOTKEY);
        assert!(!s.hide_installed);
    }

    #[test]
    fn a_blank_hotkey_falls_back_to_the_default_and_others_are_trimmed() {
        let blank = Settings { hotkey: "   ".into(), ..Settings::default() }.normalized();
        assert_eq!(blank.hotkey, DEFAULT_HOTKEY);
        let padded = Settings { hotkey: " Alt+O ".into(), ..Settings::default() }.normalized();
        assert_eq!(padded.hotkey, "Alt+O");
    }

    #[test]
    fn settings_saved_before_phase_two_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"osuExePath":null,"songsDir":null,"noVideo":false,"mirrorOrder":["catboy","osuDirect"]}"#).unwrap();
        let s = Settings::load(&path);
        assert!(!s.no_video);
        assert_eq!(s.mirror_order, vec![MirrorId::Catboy, MirrorId::OsuDirect]);
        assert_eq!(s.hotkey, DEFAULT_HOTKEY);
    }

    #[test]
    fn the_link_handler_is_off_by_default_and_remembers_no_browser() {
        let s = Settings::default();
        assert!(!s.link_handler);
        assert_eq!(s.previous_browser, None);
    }

    #[test]
    fn an_empty_remembered_browser_is_dropped() {
        let s = Settings { previous_browser: Some(PathBuf::new()), ..Settings::default() }.normalized();
        assert_eq!(s.previous_browser, None);
    }

    #[test]
    fn link_settings_round_trip_and_older_files_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let s = Settings {
            link_handler: true,
            previous_browser: Some(PathBuf::from(r"C:\Browsers\chrome.exe")),
            ..Settings::default()
        };
        s.save(&path).unwrap();
        let expected = Settings { link_handler: cfg!(windows), ..s };
        assert_eq!(Settings::load(&path), expected);

        std::fs::write(&path, r#"{"noVideo": false, "hotkey": "Alt+O"}"#).unwrap();
        let older = Settings::load(&path);
        assert!(!older.link_handler);
        assert_eq!(older.hotkey, "Alt+O");
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.json");
        let s = Settings {
            no_video: false,
            osu_exe_path: Some(PathBuf::from(r"D:\osu!\osu!.exe")),
            mirror_order: vec![MirrorId::OsuDirect, MirrorId::Catboy],
            ..Settings::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
    }

    #[test]
    fn corrupt_or_missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(&dir.path().join("nope.json")), Settings::default());
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "{ not json").unwrap();
        assert_eq!(Settings::load(&bad), Settings::default());
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{ "noVideo": false, "somethingNew": 1 }"#).unwrap();
        let s = Settings::load(&path);
        assert!(!s.no_video);
        assert_eq!(s.mirror_order, MirrorId::DEFAULT_ORDER.to_vec());
    }

    #[test]
    fn normalized_dedups_fills_in_and_drops_empty_paths() {
        let s = Settings {
            mirror_order: vec![MirrorId::OsuDirect, MirrorId::OsuDirect],
            osu_exe_path: Some(PathBuf::new()),
            ..Settings::default()
        }
        .normalized();
        assert_eq!(s.mirror_order, vec![MirrorId::OsuDirect, MirrorId::Catboy]);
        assert_eq!(s.osu_exe_path, None);
    }
}
