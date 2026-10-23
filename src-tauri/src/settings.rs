use crate::{hotkey::DEFAULT_HOTKEY, mirrors::MirrorId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// User settings, stored as JSON in the app config dir. Unknown or missing keys are fine, so
/// older and newer builds can share a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Override for `osu!.exe`; `None` means auto-detect.
    pub osu_exe_path: Option<PathBuf>,
    /// Override for the Songs folder; `None` means read it from osu!'s own config.
    pub songs_dir: Option<PathBuf>,
    pub no_video: bool,
    /// Preferred mirror order; always contains every known mirror once after `normalized`.
    pub mirror_order: Vec<MirrorId>,
    /// Global shortcut that opens the overlay, e.g. `Ctrl+Shift+D`.
    pub hotkey: String,
    /// Leave results out of the overlay when they are already in the Songs folder.
    pub hide_installed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            osu_exe_path: None,
            songs_dir: None,
            no_video: true,
            mirror_order: MirrorId::DEFAULT_ORDER.to_vec(),
            hotkey: DEFAULT_HOTKEY.to_string(),
            hide_installed: false,
        }
    }
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
        self.hotkey = match self.hotkey.trim() {
            "" => DEFAULT_HOTKEY.to_string(),
            trimmed => trimmed.to_string(),
        };
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_prefer_no_video_and_list_every_mirror() {
        let s = Settings::default();
        assert!(s.no_video);
        assert_eq!(s.mirror_order, vec![MirrorId::OsuDirect, MirrorId::Catboy]);
    }

    #[test]
    fn overlay_defaults_are_ctrl_shift_d_and_show_installed_maps() {
        let s = Settings::default();
        assert_eq!(s.hotkey, "Ctrl+Shift+D");
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
