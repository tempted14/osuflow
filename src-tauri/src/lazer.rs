//! Native lazer launching and a read-only index of available beatmap files. Never opens Realm.
use crate::{
    launcher::{self, Launch},
    osu_locator::{DetectSource, OsuInstall},
    settings::{GameClient, Settings},
};
use std::{
    collections::HashMap,
    ffi::OsStr,
    io::{self, Read},
    path::{Path, PathBuf},
};

pub fn looks_like_lazer(exe: &Path) -> bool {
    exe.is_file()
        && exe.parent().is_some_and(|dir| {
            dir.join("osu.Game.dll").is_file() || dir.join("../Resources/osu.Game.dll").is_file()
        })
}

fn normalize_app(path: PathBuf) -> PathBuf {
    if path.is_dir() && path.extension() == Some(OsStr::new("app")) {
        path.join("Contents/MacOS/osu!")
    } else {
        path
    }
}

fn app_path(path: &Path) -> bool {
    if looks_like_lazer(path) {
        return true;
    }
    #[cfg(target_os = "linux")]
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("AppImage"))
    {
        use std::os::unix::fs::PermissionsExt;
        return path.is_file()
            && path
                .metadata()
                .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0);
    }
    false
}

struct Running {
    pid: u32,
    program: PathBuf,
    source: DetectSource,
}

#[cfg(any(target_os = "linux", test))]
fn is_lazer_flatpak(text: &str) -> bool {
    let mut application = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            application = line == "[Application]";
        }
        if application
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "name"
        {
            return value.trim() == "sh.ppy.osu";
        }
    }
    false
}

/// Only native game executable names qualify; adjacent crash reporters and Wine stable do not.
fn running_clients() -> Vec<Running> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    system
        .processes()
        .values()
        .filter_map(|process| {
            let exe = process.exe()?;
            let name = exe.file_name()?.to_str()?;
            if !["osu!", "osu!.exe", "osu"]
                .iter()
                .any(|game| name.eq_ignore_ascii_case(game))
            {
                return None;
            }
            #[cfg(unix)]
            if exe.extension() == Some(OsStr::new("exe")) {
                return None;
            }
            if looks_like_lazer(exe) {
                return Some(Running {
                    pid: process.pid().as_u32(),
                    program: exe.to_path_buf(),
                    source: DetectSource::RunningProcess,
                });
            }
            #[cfg(target_os = "linux")]
            {
                // A Flatpak process exposes /app inside its own namespace. Read package identity
                // through its proc root instead of trying to execute that path on the host.
                let info = PathBuf::from(format!(
                    "/proc/{}/root/.flatpak-info",
                    process.pid().as_u32()
                ));
                if std::fs::metadata(&info).is_ok_and(|meta| meta.len() <= 16384)
                    && std::fs::read_to_string(info).is_ok_and(|text| is_lazer_flatpak(&text))
                {
                    return Some(Running {
                        pid: process.pid().as_u32(),
                        program: flatpak()?,
                        source: DetectSource::Flatpak,
                    });
                }
            }
            None
        })
        .collect()
}

pub fn running() -> Vec<(u32, PathBuf)> {
    running_clients()
        .into_iter()
        .map(|process| (process.pid, process.program))
        .collect()
}

fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    if let Some(data) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(data).join("osu"));
    }
    if let Some(home) =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
    {
        #[cfg(target_os = "linux")]
        {
            let data = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| home.join(".local/share"));
            roots.extend([data.join("osu"), home.join(".var/app/sh.ppy.osu/data/osu")]);
        }
        #[cfg(target_os = "macos")]
        roots.push(home.join("Library/Application Support/osu"));
        #[cfg(windows)]
        let _ = home;
    }
    roots
}

/// Read only the redirect key, bounded in size; ignore account/game configuration files.
pub fn redirected_data(root: &Path) -> PathBuf {
    let ini = root.join("storage.ini");
    if std::fs::metadata(&ini).is_ok_and(|meta| meta.len() <= 8192)
        && let Ok(text) = std::fs::read_to_string(ini)
        && let Some(value) = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .find_map(|(key, value)| (key.trim() == "FullPath").then(|| value.trim()))
    {
        let path = Path::new(value);
        if path.is_absolute() && path.is_dir() {
            return path.to_path_buf();
        }
    }
    root.to_path_buf()
}

pub fn data_dir(settings: &Settings) -> Option<PathBuf> {
    if let Some(path) = &settings.lazer_data_dir {
        return path.is_absolute().then(|| path.clone());
    }
    let roots = default_roots();
    roots
        .iter()
        .map(|root| redirected_data(root))
        .find(|root| root.join("files").is_dir() || root.join("client.realm").is_file())
        .or_else(|| roots.first().map(|root| redirected_data(root)))
}

fn default_app() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        let root = local.join("osulazer");
        for path in [root.join("current/osu!.exe"), root.join("osu!.exe")] {
            if looks_like_lazer(&path) {
                return Some(path);
            }
        }
        let mut versions = std::fs::read_dir(root)
            .ok()?
            .flatten()
            .take(128)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("app-"))
            })
            .collect::<Vec<_>>();
        versions.sort();
        if let Some(path) = versions
            .into_iter()
            .rev()
            .map(|dir| dir.join("osu!.exe"))
            .find(|path| looks_like_lazer(path))
        {
            return Some(path);
        }
    }
    #[cfg(target_os = "macos")]
    for path in [
        PathBuf::from("/Applications/osu!.app/Contents/MacOS/osu!"),
        std::env::var_os("HOME")
            .map(PathBuf::from)?
            .join("Applications/osu!.app/Contents/MacOS/osu!"),
    ] {
        if looks_like_lazer(&path) {
            return Some(path);
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(path) = std::env::var_os("PATH") {
        for path in
            std::env::split_paths(&path).flat_map(|dir| [dir.join("osu-lazer"), dir.join("osu!")])
        {
            if looks_like_lazer(&path) {
                return Some(path);
            }
        }
    }
    None
}

fn auto_app() -> Option<(PathBuf, DetectSource)> {
    let native = default_app().map(|path| (path, DetectSource::DefaultPath));
    #[cfg(target_os = "linux")]
    let native = native.or_else(|| flatpak().map(|path| (path, DetectSource::Flatpak)));
    native
}

#[cfg(target_os = "linux")]
fn flatpak() -> Option<PathBuf> {
    use std::process::{Command, Stdio};
    let path = std::env::var_os("PATH")?;
    let program = std::env::split_paths(&path)
        .map(|dir| dir.join("flatpak"))
        .find(|path| path.is_file())?;
    // Local package metadata only; does not install, update or start the game.
    Command::new(&program)
        .args(["info", "sh.ppy.osu"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?
        .success()
        .then_some(program)
}

pub fn detect(settings: &Settings) -> Option<OsuInstall> {
    let data = data_dir(settings)?;
    let (exe, source) = if settings.lazer_launch_command.is_some() {
        (
            settings
                .lazer_exe_path
                .clone()
                .map(normalize_app)
                .unwrap_or_default(),
            DetectSource::CustomCommand,
        )
    } else if let Some(path) = settings.lazer_exe_path.clone() {
        let path = normalize_app(path);
        if !app_path(&path) {
            return None;
        }
        (path, DetectSource::Settings)
    } else if let Some(process) = running_clients().into_iter().next() {
        (process.program, process.source)
    } else {
        auto_app()?
    };
    #[cfg(target_os = "linux")]
    let data = if source == DetectSource::Flatpak && settings.lazer_data_dir.is_none() {
        redirected_data(
            &PathBuf::from(std::env::var_os("HOME")?).join(".var/app/sh.ppy.osu/data/osu"),
        )
    } else {
        data
    };
    let dir = exe
        .parent()
        .filter(|dir| dir.is_dir())
        .map(Path::to_path_buf)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
        })?;
    Some(OsuInstall {
        client: GameClient::Lazer,
        exe,
        dir,
        songs_dir: data.join("files"),
        exclusive_fullscreen: false,
        source,
    })
}

pub fn launch_plan(
    install: &OsuInstall,
    archive: &Path,
    settings: &Settings,
) -> io::Result<Launch> {
    if let Some(command) = &settings.lazer_launch_command {
        return launcher::custom_launch(command, archive, None);
    }
    if install.client == GameClient::Lazer && install.source == DetectSource::Flatpak {
        return Ok(Launch {
            program: install.exe.as_os_str().to_owned(),
            args: vec![
                "run".into(),
                "--file-forwarding".into(),
                "sh.ppy.osu".into(),
                "@@".into(),
                archive.as_os_str().to_owned(),
                "@@".into(),
            ],
            prefix: None,
        });
    }
    if install.client != GameClient::Lazer || !app_path(&install.exe) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Choose the osu!lazer application in Settings",
        ));
    }
    Ok(Launch {
        program: install.exe.as_os_str().to_owned(),
        args: vec![archive.as_os_str().to_owned()],
        prefix: None,
    })
}

pub fn launch(install: &OsuInstall, archive: &Path, settings: &Settings) -> io::Result<()> {
    launcher::run(launch_plan(install, archive, settings)?, &install.dir)
}

/// A stored lazer file is SHA-256 addressed: files/a/ab/abcdef... (64 hex digits).
pub fn stored_file(root: &Path, file: &Path) -> bool {
    let Ok(relative) = file.strip_prefix(root) else {
        return false;
    };
    let parts = relative
        .components()
        .map(|part| part.as_os_str().to_str())
        .collect::<Option<Vec<_>>>();
    let Some(parts) = parts else {
        return false;
    };
    parts.len() == 3
        && parts[0].len() == 1
        && parts[1].len() == 2
        && parts[2].len() == 64
        && parts[2].bytes().all(|byte| byte.is_ascii_hexdigit())
        && parts[2].starts_with(parts[1])
        && parts[1].starts_with(parts[0])
}

pub fn file_set_id(file: &Path) -> Option<u32> {
    let mut input = std::fs::File::open(file).ok()?;
    let mut prefix = [0; 256];
    let count = input.read(&mut prefix).ok()?;
    let header = String::from_utf8_lossy(&prefix[..count]);
    if !header
        .trim_start_matches('\u{feff}')
        .starts_with("osu file format v")
    {
        return None;
    }
    let mut bytes = prefix[..count].to_vec();
    input
        .take(65536 - count as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut metadata = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            metadata = line == "[Metadata]";
        }
        if metadata
            && let Some((key, value)) = line.split_once(':')
            && key.trim() == "BeatmapSetID"
        {
            return value.trim().parse::<u32>().ok().filter(|id| *id > 0);
        }
    }
    None
}

pub fn scan_files(root: &Path) -> HashMap<PathBuf, u32> {
    fn scan(root: &Path, dir: &Path, depth: usize, files: &mut HashMap<PathBuf, u32>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir()
                && depth < 2
                && entry.file_name().to_str().is_some_and(|name| {
                    name.len() == depth + 1 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
            {
                scan(root, &path, depth + 1, files);
            } else if kind.is_file()
                && stored_file(root, &path)
                && let Some(id) = file_set_id(&path)
            {
                files.insert(path, id);
            }
        }
    }
    let mut files = HashMap::new();
    scan(root, root, 0, &mut files);
    files
}

#[cfg(test)]
pub(crate) fn fixture_file(root: &Path, hash: &str, id: u32) -> PathBuf {
    let file = root.join(&hash[..1]).join(&hash[..2]).join(hash);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, format!("\u{feff}osu file format v14\n\n[General]\nMode:0\n[Metadata]\nBeatmapSetID:{id}\n[HitObjects]\n")).unwrap();
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn native(root: &Path) -> OsuInstall {
        let exe = root.join("osu!.exe");
        std::fs::write(&exe, b"fake").unwrap();
        std::fs::write(root.join("osu.Game.dll"), b"fake").unwrap();
        OsuInstall {
            client: GameClient::Lazer,
            exe,
            dir: root.to_path_buf(),
            songs_dir: root.join("files"),
            exclusive_fullscreen: false,
            source: DetectSource::Settings,
        }
    }

    #[test]
    fn native_lazer_is_distinct_from_stable_and_uses_a_single_archive_argument() {
        let root = tempfile::tempdir().unwrap();
        let game = native(root.path());
        assert!(looks_like_lazer(&game.exe));
        assert!(!crate::osu_locator::looks_like_stable(&game.exe));
        let archive = root.path().join("123 曲; $(touch nope).osz");
        let plan = launch_plan(&game, &archive, &Settings::default()).unwrap();
        assert_eq!(plan.program, game.exe);
        assert_eq!(plan.args, vec![archive.as_os_str().to_owned()]);
        assert!(plan.prefix.is_none());
        let stable = OsuInstall {
            client: GameClient::Stable,
            ..game
        };
        assert!(launch_plan(&stable, &archive, &Settings::default()).is_err());
    }

    #[test]
    fn flatpak_exports_the_archive_through_the_document_portal_without_shell_parsing() {
        let root = tempfile::tempdir().unwrap();
        let mut game = native(root.path());
        game.exe = "flatpak".into();
        game.source = DetectSource::Flatpak;
        let archive = root.path().join("123 a b 曲.osz");
        let plan = launch_plan(&game, &archive, &Settings::default()).unwrap();
        assert_eq!(
            plan.args,
            vec![
                OsString::from("run"),
                "--file-forwarding".into(),
                "sh.ppy.osu".into(),
                "@@".into(),
                archive.into_os_string(),
                "@@".into()
            ]
        );
    }

    #[test]
    fn flatpak_identity_must_name_lazer_in_the_application_section() {
        assert!(is_lazer_flatpak(
            "[Application]\nname=sh.ppy.osu\n[Instance]\nname=ignored"
        ));
        assert!(!is_lazer_flatpak("[Instance]\nname=sh.ppy.osu"));
        assert!(!is_lazer_flatpak("[Application]\nname=another.app"));
    }

    #[test]
    fn custom_native_launcher_never_inherits_a_stable_prefix_override() {
        let root = tempfile::tempdir().unwrap();
        let game = native(root.path());
        let settings = Settings {
            wine_prefix: Some(root.path().join("stable-prefix")),
            lazer_launch_command: Some("'/Apps/Custom launcher' '{file}'".into()),
            ..Settings::default()
        };
        let plan = launch_plan(&game, Path::new("map.osz"), &settings).unwrap();
        assert_eq!(plan.program, "/Apps/Custom launcher");
        assert_eq!(plan.args, vec![OsString::from("map.osz")]);
        assert!(plan.prefix.is_none());
    }

    #[test]
    fn storage_redirect_is_bounded_and_reads_only_an_existing_absolute_fullpath() {
        let root = tempfile::tempdir().unwrap();
        let moved = root.path().join("Moved data");
        std::fs::create_dir(&moved).unwrap();
        let ini = root.path().join("storage.ini");
        std::fs::write(
            &ini,
            format!("Unknown=ignored\nFullPath = {}\n", moved.display()),
        )
        .unwrap();
        assert_eq!(redirected_data(root.path()), moved);
        std::fs::write(&ini, "FullPath=relative/path").unwrap();
        assert_eq!(redirected_data(root.path()), root.path());
        std::fs::write(&ini, "x".repeat(8193)).unwrap();
        assert_eq!(redirected_data(root.path()), root.path());
    }

    #[test]
    fn hashed_library_scans_only_beatmap_headers_and_never_opens_or_writes_realm() {
        let root = tempfile::tempdir().unwrap();
        let files = root.path().join("files");
        let map = fixture_file(&files, &"a".repeat(64), 123);
        let duplicate = fixture_file(&files, &"b".repeat(64), 123);
        let media = fixture_file(&files, &"c".repeat(64), 999);
        std::fs::write(&media, [0, 1, 2, 3]).unwrap();
        std::fs::write(
            root.path().join("client.realm"),
            b"never open this database",
        )
        .unwrap();
        let index = scan_files(&files);
        assert_eq!(index, HashMap::from([(map, 123), (duplicate, 123)]));
        assert_eq!(
            std::fs::read(root.path().join("client.realm")).unwrap(),
            b"never open this database"
        );
        assert!(!stored_file(&files, &files.join("../outside")));
        assert!(!stored_file(&files, &files.join("a/aa/not-a-hash")));
    }

    #[test]
    fn metadata_ids_must_be_positive_and_inside_metadata() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("map");
        for text in [
            "not osu\n[Metadata]\nBeatmapSetID:12",
            "osu file format v14\n[General]\nBeatmapSetID:12",
            "osu file format v14\n[Metadata]\nBeatmapSetID:-1",
        ] {
            std::fs::write(&path, text).unwrap();
            assert_eq!(file_set_id(&path), None);
        }
        std::fs::write(
            &path,
            "osu file format v14\n[Metadata]\nBeatmapSetID : 42\n[HitObjects]\nBeatmapSetID:99",
        )
        .unwrap();
        assert_eq!(file_set_id(&path), Some(42));
    }
}
