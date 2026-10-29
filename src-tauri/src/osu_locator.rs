//! Finds the osu! stable install: where `osu!.exe` is, which folder holds the maps, and whether
//! the game is in exclusive fullscreen. Only reads the install folder and osu!'s own config; the
//! config can contain account details, so only the handful of keys needed are ever looked at.

use crate::settings::Settings;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DetectSource {
    Settings,
    RunningProcess,
    Registry,
    DefaultPath,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OsuInstall {
    pub exe: PathBuf,
    pub dir: PathBuf,
    pub songs_dir: PathBuf,
    /// osu!'s `Fullscreen = 1`. Overlays can't draw over exclusive fullscreen.
    pub exclusive_fullscreen: bool,
    pub source: DetectSource,
}

/// Settings override, then a running osu!, then the .osz file association, then the default
/// install folder. osu!lazer is skipped: it is also called `osu!.exe`.
pub fn detect(settings: &Settings) -> Option<OsuInstall> {
    let songs_override = settings.songs_dir.clone();
    let username = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" }).ok();
    let build = |exe: PathBuf, source: DetectSource| {
        build_install(exe, songs_override.clone(), source, username.as_deref(), settings.wine_prefix.as_deref())
    };

    if let Some(exe) = settings.osu_exe_path.clone().filter(|p| p.is_file() && (cfg!(windows) || looks_like_stable(p))) {
        return Some(build(exe, DetectSource::Settings));
    }
    #[cfg(unix)]
    if settings.wine_prefix.is_some() {
        return crate::wine::find_install(settings).map(|exe| build(exe, DetectSource::Settings));
    }
    if let Some(exe) = running_osu_exe() {
        return Some(build(exe, DetectSource::RunningProcess));
    }
    if let Some(exe) = registry_osu_exe() {
        return Some(build(exe, DetectSource::Registry));
    }
    default_osu_exe().map(|exe| build(exe, DetectSource::DefaultPath))
}

/// True for a stable install folder, false for lazer (has `osu.Game.dll`) or an empty folder.
pub fn looks_like_stable(exe: &Path) -> bool {
    let Some(dir) = exe.parent() else { return false };
    if !exe.is_file() || dir.join("osu.Game.dll").exists() {
        return false;
    }
    ["osu!.db", "osu!.cfg", "osu!auth.dll", "osu!gameplay.dll", "Songs"]
        .iter()
        .any(|name| dir.join(name).exists())
}

/// Process id and executable of every running osu! stable (lazer is skipped).
#[cfg(windows)]
pub fn running_osu_processes() -> Vec<(u32, PathBuf)> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    system
        .processes_by_exact_name(std::ffi::OsStr::new("osu!.exe"))
        .filter_map(|process| Some((process.pid().as_u32(), process.exe()?.to_path_buf())))
        .filter(|(_, exe)| looks_like_stable(exe))
        .collect()
}

#[cfg(unix)]
pub fn running_osu_processes() -> Vec<(u32, PathBuf)> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet).with_cwd(UpdateKind::OnlyIfNotSet)
            .with_environ(UpdateKind::OnlyIfNotSet));
    let known = crate::wine::prefixes();
    system.processes().values().filter_map(|process| {
        let mut prefixes = known.clone();
        // Only this key is used; no process environment is logged or sent to the frontend.
        if let Some(prefix) = process.environ().iter().find_map(|value| value.to_str()?.strip_prefix("WINEPREFIX=")) {
            prefixes.insert(0, PathBuf::from(prefix));
        }
        crate::wine::process_exe(process.name(), process.exe(), process.cmd(), process.cwd(), &prefixes)
            .map(|exe| (process.pid().as_u32(), exe))
    }).collect()
}

fn running_osu_exe() -> Option<PathBuf> {
    running_osu_processes().into_iter().next().map(|(_, exe)| exe)
}

#[cfg(windows)]
fn registry_osu_exe() -> Option<PathBuf> {
    use winreg::{RegKey, enums::HKEY_CLASSES_ROOT};
    let classes = RegKey::predef(HKEY_CLASSES_ROOT);
    [
        r"osustable.File.osz\shell\open\command",
        r"osu\shell\open\command",
    ]
    .iter()
    .filter_map(|key| {
        let command: String = classes.open_subkey(key).ok()?.get_value("").ok()?;
        exe_from_shell_command(&command)
    })
    .find(|exe| looks_like_stable(exe))
}

#[cfg(not(windows))]
fn registry_osu_exe() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn default_osu_exe() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("osu!").join("osu!.exe")).filter(|exe| looks_like_stable(exe))
}

#[cfg(unix)]
fn default_osu_exe() -> Option<PathBuf> {
    crate::wine::find_install(&Settings::default())
}

fn build_install(
    exe: PathBuf,
    songs_override: Option<PathBuf>,
    source: DetectSource,
    username: Option<&str>,
    wine_prefix: Option<&Path>,
) -> OsuInstall {
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let cfg = cfg_path_for(&dir, username)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse_cfg(&text))
        .unwrap_or_default();
    let songs_dir = songs_override.unwrap_or_else(|| {
        #[cfg(unix)]
        if let Some(prefix) = wine_prefix {
            return crate::wine::songs_path(&dir, cfg.get("BeatmapDirectory").map(String::as_str), Some(prefix));
        }
        #[cfg(windows)]
        let _ = wine_prefix;
        songs_dir_from_cfg(&dir, &cfg)
    });
    OsuInstall {
        songs_dir,
        exclusive_fullscreen: cfg.get("Fullscreen").is_some_and(|v| v == "1"),
        exe,
        dir,
        source,
    }
}

/// `osu!.<windows user>.cfg` when present, else the most recently written `osu!.*.cfg`.
/// The bare `osu!.cfg` holds no per-user settings.
fn cfg_path_for(osu_dir: &Path, username: Option<&str>) -> Option<PathBuf> {
    if let Some(user) = username {
        let own = osu_dir.join(format!("osu!.{user}.cfg"));
        if own.is_file() {
            return Some(own);
        }
    }
    std::fs::read_dir(osu_dir)
        .ok()?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("osu!.") && name.ends_with(".cfg") && name != "osu!.cfg"
        })
        .max_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok())
        .map(|entry| entry.path())
}

/// `Key = Value` lines; blank lines and `#` comments are skipped.
pub fn parse_cfg(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

/// `BeatmapDirectory` is relative to the install folder unless it is an absolute path.
pub fn songs_dir_from_cfg(osu_dir: &Path, cfg: &HashMap<String, String>) -> PathBuf {
    #[cfg(unix)]
    {
        let prefix = crate::wine::prefix_for_exe(osu_dir).or_else(|| std::env::var_os("WINEPREFIX").map(PathBuf::from));
        crate::wine::songs_path(osu_dir, cfg.get("BeatmapDirectory").map(String::as_str), prefix.as_deref())
    }
    #[cfg(windows)]
    { windows_songs_dir_from_cfg(osu_dir, cfg) }
}

#[cfg(windows)]
fn windows_songs_dir_from_cfg(osu_dir: &Path, cfg: &HashMap<String, String>) -> PathBuf {
    match cfg.get("BeatmapDirectory").map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(dir) => {
            let dir = Path::new(dir);
            if dir.is_absolute() { dir.to_path_buf() } else { osu_dir.join(dir) }
        }
        None => osu_dir.join("Songs"),
    }
}

/// The executable out of a registry shell command such as `"C:\osu!\osu!.exe" "%1"`.
#[cfg(any(windows, test))]
pub fn exe_from_shell_command(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(PathBuf::from(&rest[..end])).filter(|p| !p.as_os_str().is_empty());
    }
    let end = command.to_ascii_lowercase().find(".exe")? + 4;
    Some(PathBuf::from(&command[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cfg_lines() {
        let cfg = parse_cfg("# comment\r\nBeatmapDirectory = Songs\r\nFullscreen = 0\r\n\r\nKey=a=b\r\nnot a setting\r\n");
        assert_eq!(cfg["BeatmapDirectory"], "Songs");
        assert_eq!(cfg["Fullscreen"], "0");
        assert_eq!(cfg["Key"], "a=b");
        assert_eq!(cfg.len(), 3);
    }

    #[cfg(windows)]
    #[test]
    fn songs_dir_is_relative_absolute_or_default() {
        let dir = Path::new(r"C:\Games\osu!");
        let with = |value: &str| {
            songs_dir_from_cfg(dir, &HashMap::from([("BeatmapDirectory".to_string(), value.to_string())]))
        };
        assert_eq!(with("Songs"), dir.join("Songs"));
        assert_eq!(with(r"D:\Maps"), PathBuf::from(r"D:\Maps"));
        assert_eq!(with("  "), dir.join("Songs"));
        assert_eq!(songs_dir_from_cfg(dir, &HashMap::new()), dir.join("Songs"));
    }

    #[test]
    fn extracts_the_exe_from_shell_commands() {
        assert_eq!(
            exe_from_shell_command(r#""C:\Users\a\AppData\Local\osu!\osu!.exe" "%1""#),
            Some(PathBuf::from(r"C:\Users\a\AppData\Local\osu!\osu!.exe"))
        );
        assert_eq!(
            exe_from_shell_command(r#"C:\Program Files\osu!\OSU!.EXE "%1""#),
            Some(PathBuf::from(r"C:\Program Files\osu!\OSU!.EXE"))
        );
        assert_eq!(exe_from_shell_command(r#""" "%1""#), None);
        assert_eq!(exe_from_shell_command("rundll32 something"), None);
    }

    #[test]
    fn picks_the_users_cfg_then_the_newest_other_one() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["osu!.cfg", "osu!.alice.cfg", "osu!.bob.cfg", "notes.txt"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        let picked = |user: Option<&str>| {
            cfg_path_for(dir.path(), user).map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        };
        assert_eq!(picked(Some("alice")).as_deref(), Some("osu!.alice.cfg"));
        assert_eq!(picked(Some("bob")).as_deref(), Some("osu!.bob.cfg"));
        // Unknown user: any per-user cfg is better than none, but never the bare osu!.cfg.
        assert!(matches!(picked(Some("carol")).as_deref(), Some("osu!.alice.cfg" | "osu!.bob.cfg")));
        let empty = tempfile::tempdir().unwrap();
        std::fs::write(empty.path().join("osu!.cfg"), "").unwrap();
        assert_eq!(cfg_path_for(empty.path(), Some("alice")), None);
    }

    #[test]
    fn stable_is_told_apart_from_lazer_and_empty_folders() {
        let stable = tempfile::tempdir().unwrap();
        std::fs::write(stable.path().join("osu!.exe"), "").unwrap();
        assert!(!looks_like_stable(&stable.path().join("osu!.exe")), "no stable files yet");
        std::fs::write(stable.path().join("osu!.db"), "").unwrap();
        assert!(looks_like_stable(&stable.path().join("osu!.exe")));

        let lazer = tempfile::tempdir().unwrap();
        std::fs::write(lazer.path().join("osu!.exe"), "").unwrap();
        std::fs::write(lazer.path().join("osu.Game.dll"), "").unwrap();
        std::fs::write(lazer.path().join("osu!.db"), "").unwrap();
        assert!(!looks_like_stable(&lazer.path().join("osu!.exe")));

        assert!(!looks_like_stable(&stable.path().join("missing.exe")));
    }

    #[test]
    fn builds_an_install_from_the_users_cfg() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("osu!.exe");
        std::fs::write(&exe, "").unwrap();
        std::fs::write(
            dir.path().join("osu!.alice.cfg"),
            "BeatmapDirectory = Maps\nFullscreen = 1\n",
        )
        .unwrap();

        let install = build_install(exe.clone(), None, DetectSource::DefaultPath, Some("alice"), None);
        assert_eq!(install.songs_dir, dir.path().join("Maps"));
        assert!(install.exclusive_fullscreen);
        assert_eq!(install.dir, dir.path());

        let overridden =
            build_install(exe, Some(PathBuf::from(r"E:\Songs")), DetectSource::Settings, Some("alice"), None);
        assert_eq!(overridden.songs_dir, PathBuf::from(r"E:\Songs"));
    }

    #[test]
    fn missing_cfg_means_default_songs_dir_and_not_fullscreen() {
        let dir = tempfile::tempdir().unwrap();
        let install = build_install(dir.path().join("osu!.exe"), None, DetectSource::DefaultPath, Some("x"), None);
        assert_eq!(install.songs_dir, dir.path().join("Songs"));
        assert!(!install.exclusive_fullscreen);
    }
}
