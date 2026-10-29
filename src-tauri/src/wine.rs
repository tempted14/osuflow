//! Host-side Wine discovery and argument-safe import commands. No shell or game injection.
use crate::{osu_locator::{OsuInstall, looks_like_stable}, settings::Settings};
use std::{ffi::{OsStr, OsString}, io, path::{Path, PathBuf}};

/// Translate a Wine drive path through its actual dosdevices mapping. Z: normally maps to /.
pub fn host_path(value: &str, prefix: Option<&Path>) -> Option<PathBuf> {
    let value = value.trim().trim_matches('"').replace('\\', "/");
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        if bytes.get(2) != Some(&b'/') { return None; } // C:relative depends on a Wine working directory.
        let drive = char::from(bytes[0].to_ascii_lowercase());
        let rest = value[3..].trim_start_matches('/');
        #[cfg(unix)]
        if let Some(prefix) = prefix {
            let device = prefix.join("dosdevices").join(format!("{drive}:"));
            if let Ok(mapped) = std::fs::canonicalize(&device) { return Some(mapped.join(rest)); }
        }
        return match drive {
            'z' => Some(Path::new("/").join(rest)),
            'c' => prefix.map(|prefix| prefix.join("drive_c").join(rest)),
            _ => None,
        };
    }
    if value.is_empty() || value.starts_with("//") { None } else { Some(PathBuf::from(value)) }
}

pub fn prefix_for_exe(exe: &Path) -> Option<PathBuf> {
    exe.ancestors().find(|path| path.file_name() == Some(OsStr::new("drive_c")))?.parent().map(Path::to_path_buf)
}

pub fn songs_path(dir: &Path, configured: Option<&str>, prefix: Option<&Path>) -> PathBuf {
    let Some(value) = configured.map(str::trim).filter(|value| !value.is_empty()) else { return dir.join("Songs") };
    match host_path(value, prefix) {
        Some(path) if path.is_absolute() || value.starts_with('/') => path,
        Some(path) if value.as_bytes().get(1).is_none_or(|byte| *byte != b':') => dir.join(path),
        Some(path) => path,
        None => dir.join("Songs"),
    }
}

/// Only known install locations are inspected; never recursively search a user's home directory.
pub fn install_in_prefix(prefix: &Path) -> Option<PathBuf> {
    let drive = prefix.join("drive_c");
    let mut candidates = vec![drive.join("osu!/osu!.exe"), drive.join("osu/osu!.exe"),
        drive.join("Games/osu!/osu!.exe"), drive.join("Program Files/osu!/osu!.exe"),
        drive.join("Program Files (x86)/osu!/osu!.exe")];
    if let Ok(users) = std::fs::read_dir(drive.join("users")) {
        for user in users.flatten().take(128).filter(|entry| entry.path().is_dir()) {
            candidates.push(user.path().join("AppData/Local/osu!/osu!.exe"));
            candidates.push(user.path().join("Local Settings/Application Data/osu!/osu!.exe"));
        }
    }
    candidates.into_iter().find(|exe| looks_like_stable(exe))
}

#[cfg(unix)]
pub fn prefixes() -> Vec<PathBuf> {
    let mut prefixes = Vec::new();
    if let Some(prefix) = std::env::var_os("WINEPREFIX") { prefixes.push(PathBuf::from(prefix)); }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let data = data_home(&home);
        prefixes.extend([data.join("wineprefixes/osu-wineprefix"), data.join("osuwinello"), home.join(".wine")]);
        #[cfg(target_os = "macos")]
        for root in [home.join("Library/Application Support/CrossOver/Bottles"), home.join("Applications/Wineskin")] {
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten().take(128).filter(|entry| entry.path().is_dir()) {
                    prefixes.push(entry.path());
                    prefixes.push(entry.path().join("Contents/SharedSupport/prefix"));
                    prefixes.push(entry.path().join("Contents/Resources"));
                }
            }
        }
    }
    prefixes
}

#[cfg(unix)]
fn data_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"))
}

#[cfg(unix)]
pub fn find_install(settings: &Settings) -> Option<PathBuf> {
    if let Some(prefix) = &settings.wine_prefix {
        return install_in_prefix(prefix); // An explicit prefix must not silently select another install.
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        // osu-winello records the selected game folder here; it is not fixed inside the Wine prefix.
        let path = data_home(&home).join("osuconfig/osupath");
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() <= 4096)
            && let Ok(value) = std::fs::read_to_string(path)
        {
            let exe = Path::new(value.trim()).join("osu!.exe");
            if looks_like_stable(&exe) { return Some(exe); }
        }
    }
    prefixes().into_iter().find_map(|prefix| install_in_prefix(&prefix))
}

/// Resolve only a process that names osu!.exe. A Wine loader or unrelated .exe never counts as osu!.
pub fn process_exe(name: &OsStr, exe: Option<&Path>, args: &[OsString], cwd: Option<&Path>, prefixes: &[PathBuf]) -> Option<PathBuf> {
    let is_osu = |value: &str| value.replace('\\', "/").rsplit('/').next().is_some_and(|name| name.eq_ignore_ascii_case("osu!.exe"));
    let named_osu = is_osu(&name.to_string_lossy());
    let wine_loader = exe.and_then(Path::file_name).is_some_and(|name| name.to_string_lossy().to_ascii_lowercase().contains("wine"));
    if !named_osu && !wine_loader { return None; }
    let mut candidates = Vec::new();
    if let Some(exe) = exe.filter(|exe| is_osu(&exe.to_string_lossy())) { candidates.push(exe.to_path_buf()); }
    for arg in args.iter().filter(|arg| is_osu(&arg.to_string_lossy())) {
        let value = arg.to_string_lossy();
        for prefix in prefixes.iter().map(Some).chain(std::iter::once(None)) {
            if let Some(path) = host_path(&value, prefix.map(PathBuf::as_path)) {
                candidates.push(if path.is_absolute() { path } else { cwd.unwrap_or(Path::new("/")).join(path) });
            }
        }
    }
    if named_osu && let Some(cwd) = cwd { candidates.push(cwd.join("osu!.exe")); }
    candidates.into_iter().find(|exe| looks_like_stable(exe)).map(|exe| std::fs::canonicalize(&exe).unwrap_or(exe))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub prefix: Option<PathBuf>,
}

/// A custom command is parsed into argv; {file} is substituted as one argument, never shell code.
pub fn custom_launch(command: &str, archive: &Path, prefix: Option<PathBuf>) -> io::Result<Launch> {
    let mut words = shlex::split(command).filter(|words| !words.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Launch command has unmatched quotes or is empty"))?;
    let program = words.remove(0);
    if program.contains("{file}") { return Err(io::Error::new(io::ErrorKind::InvalidInput, "{file} belongs in a launch argument, not the program name")); }
    let mut has_file = false;
    let mut args = words.into_iter().map(|word| {
        if word.contains("{file}") { has_file = true; }
        if word == "{file}" { archive.as_os_str().to_owned() } else { OsString::from(word.replace("{file}", &archive.to_string_lossy())) }
    }).collect::<Vec<_>>();
    if !has_file { args.push(archive.as_os_str().to_owned()); }
    Ok(Launch { program: program.into(), args, prefix })
}

pub fn launch_plan(install: &OsuInstall, archive: &Path, settings: &Settings, osu_wine: Option<OsString>, wine: Option<OsString>) -> io::Result<Vec<Launch>> {
    let prefix = settings.wine_prefix.clone().or_else(|| prefix_for_exe(&install.exe));
    if let Some(command) = &settings.osu_launch_command { return Ok(vec![custom_launch(command, archive, prefix)?]); }
    let mut launches = Vec::new();
    // osu-winello owns its game directory and prefix. An explicit prefix uses Wine directly.
    if settings.wine_prefix.is_none() && let Some(program) = osu_wine {
        launches.push(Launch { program, args: vec!["--osuhandler".into(), archive.as_os_str().to_owned()], prefix: prefix.clone() });
    }
    if let Some(program) = wine {
        launches.push(Launch { program, args: vec!["start".into(), "/unix".into(), archive.as_os_str().to_owned()], prefix });
    }
    Ok(launches)
}

#[cfg(unix)]
fn program_on_path(name: &str) -> Option<OsString> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(name))
        .find(|path| path.is_file() && path.metadata().is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0))
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path).into_os_string())
}

#[cfg(unix)]
pub fn launch(install: &OsuInstall, archive: &Path, settings: &Settings) -> io::Result<()> {
    use std::{process::{Command, Stdio}, time::{Duration, Instant}};
    if !looks_like_stable(&install.exe) { return Err(io::Error::new(io::ErrorKind::NotFound, "Choose an osu! stable executable on the host filesystem")); }
    if settings.wine_prefix.as_ref().is_some_and(|prefix| !prefix.is_absolute() || !prefix.is_dir()) {
        return Err(io::Error::new(io::ErrorKind::NotFound, "The configured Wine prefix must be an existing absolute host path"));
    }
    let mut error = io::Error::new(io::ErrorKind::NotFound, "No osu-wine or Wine launcher found. Choose a launch command, or set the Songs folder and press F5 in osu! song select after downloading");
    for launch in launch_plan(install, archive, settings, program_on_path("osu-wine"), program_on_path("wine").or_else(|| program_on_path("wine64")))? {
        let mut command = Command::new(launch.program);
        command.args(launch.args).current_dir(&install.dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if let Some(prefix) = launch.prefix { command.env("WINEPREFIX", prefix); }
        let mut child = match command.spawn() { Ok(child) => child, Err(failure) => { error = failure; continue; } };
        // Helpers usually exit promptly. Some wrappers remain attached to the game: do not kill them.
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => { error = io::Error::other(format!("The Wine import helper exited with {status}")); break; }
                Err(failure) => { error = failure; break; }
                Ok(None) if started.elapsed() >= Duration::from_secs(2) => {
                    std::thread::spawn(move || { let _ = child.wait(); });
                    return Ok(()); // Handoff still verifies that osu! takes the archive.
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            }
        }
    }
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osu_locator::DetectSource;
    fn stable(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir.join("Songs")).unwrap();
        let exe = dir.join("osu!.exe"); std::fs::write(&exe, b"fake").unwrap(); exe
    }
    fn install(root: &Path) -> OsuInstall {
        let dir = root.join("prefix/drive_c/osu!"); let exe = stable(&dir);
        OsuInstall { exe, songs_dir: dir.join("Songs"), dir, exclusive_fullscreen: false, source: DetectSource::Settings }
    }
    #[test]
    fn translates_drives_and_relative_songs_without_inventing_unknown_drives() {
        let root = tempfile::tempdir().unwrap(); let prefix = root.path().join("prefix");
        assert_eq!(host_path(r"C:\Games\osu!", Some(&prefix)), Some(prefix.join("drive_c/Games/osu!")));
        assert_eq!(host_path(r"Z:\home\player\Maps", None), Some(PathBuf::from("/home/player/Maps")));
        assert_eq!(host_path(r"D:\Maps", Some(&prefix)), None);
        assert_eq!(host_path("C:relative", Some(&prefix)), None);
        assert_eq!(songs_path(root.path(), Some(r"Maps\Ranked"), None), root.path().join("Maps/Ranked"));
        assert_eq!(songs_path(root.path(), Some(r"C:\Maps"), Some(&prefix)), prefix.join("drive_c/Maps"));
        assert_eq!(songs_path(root.path(), None, None), root.path().join("Songs"));
    }
    #[cfg(unix)]
    #[test]
    fn honors_a_custom_drive_mapping() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("dosdevices")).unwrap();
        let maps = root.path().join("Mounted Maps"); std::fs::create_dir(&maps).unwrap();
        symlink(&maps, root.path().join("dosdevices/d:")).unwrap();
        assert_eq!(host_path(r"D:\Songs", Some(root.path())), Some(std::fs::canonicalize(&maps).unwrap().join("Songs")));
    }
    #[test]
    fn finds_a_stable_install_in_a_prefix_and_ignores_lazer() {
        let root = tempfile::tempdir().unwrap(); let game = install(root.path());
        let prefix = root.path().join("prefix");
        assert_eq!(prefix_for_exe(&game.exe), Some(prefix.clone()));
        assert_eq!(install_in_prefix(&prefix), Some(game.exe.clone()));
        std::fs::write(game.dir.join("osu.Game.dll"), b"").unwrap();
        assert_eq!(install_in_prefix(&prefix), None);
    }
    #[test]
    fn resolves_wine_process_arguments_but_not_the_loader_or_other_games() {
        let root = tempfile::tempdir().unwrap(); let game = install(root.path());
        let prefixes = vec![root.path().join("prefix")];
        let loader = Path::new("/usr/bin/wine-preloader");
        assert_eq!(process_exe(OsStr::new("wine-preloader"), Some(loader), &[r"C:\osu!\osu!.exe".into()], None, &prefixes), std::fs::canonicalize(&game.exe).ok());
        assert_eq!(process_exe(OsStr::new("osu!.exe"), Some(loader), &[], Some(&game.dir), &prefixes), std::fs::canonicalize(&game.exe).ok());
        assert_eq!(process_exe(OsStr::new("wine-preloader"), Some(loader), &["other.exe".into()], Some(&game.dir), &prefixes), None);
    }
    #[test]
    fn launch_arguments_keep_spaces_unicode_and_metacharacters_in_one_file_argument() {
        let root = tempfile::tempdir().unwrap(); let game = install(root.path());
        let archive = root.path().join("123 曲; $(touch nope).osz");
        let plan = launch_plan(&game, &archive, &Settings::default(), Some("osu-wine".into()), Some("wine".into())).unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].program, "osu-wine");
        assert_eq!(plan[0].args, vec![OsString::from("--osuhandler"), archive.as_os_str().to_owned()]);
        assert_eq!(plan[1].args, vec![OsString::from("start"), "/unix".into(), archive.as_os_str().to_owned()]);
        assert_eq!(plan[1].prefix, Some(root.path().join("prefix")));
        let custom = custom_launch("'/opt/Wine Wrapper/launch' --import '{file}'", &archive, None).unwrap();
        assert_eq!(custom.program, "/opt/Wine Wrapper/launch");
        assert_eq!(custom.args, vec![OsString::from("--import"), archive.as_os_str().to_owned()]);
        assert_eq!(custom_launch("helper --import", &archive, None).unwrap().args.last(), Some(&archive.as_os_str().to_owned()));
        assert!(custom_launch("'unterminated", &archive, None).is_err());
        assert!(custom_launch("{file} --import", &archive, None).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn a_custom_helper_gets_the_exact_archive_and_prefix_and_reports_failure() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap(); let game = install(root.path());
        let helper = root.path().join("fake helper"); let seen = root.path().join("seen.txt");
        std::fs::write(&helper, "#!/bin/sh\nprintf '%s\\n%s\\n' \"$WINEPREFIX\" \"$1\" > \"$2\"\n").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let archive = root.path().join("123 曲; $(touch nope).osz"); std::fs::write(&archive, b"PK").unwrap();
        let prefix = root.path().join("prefix");
        let settings = Settings { wine_prefix: Some(prefix.clone()), osu_launch_command: Some(format!("'{}' '{{file}}' '{}'", helper.display(), seen.display())), ..Settings::default() };
        launch(&game, &archive, &settings).unwrap();
        assert_eq!(std::fs::read_to_string(&seen).unwrap(), format!("{}\n{}\n", prefix.display(), archive.display()));
        assert!(archive.exists()); assert!(!root.path().join("nope").exists());
        std::fs::write(&helper, "#!/bin/sh\nexit 7\n").unwrap();
        assert!(launch(&game, &archive, &settings).is_err());
    }

    #[test]
    fn explicit_prefix_or_command_cannot_route_to_another_winello_install() {
        let root = tempfile::tempdir().unwrap(); let game = install(root.path());
        let settings = Settings { wine_prefix: Some(root.path().join("selected")), ..Settings::default() };
        let plan = launch_plan(&game, Path::new("map.osz"), &settings, Some("osu-wine".into()), Some("wine".into())).unwrap();
        assert_eq!(plan.len(), 1); assert_eq!(plan[0].program, "wine"); assert_eq!(plan[0].prefix, settings.wine_prefix);
        let settings = Settings { osu_launch_command: Some("wrapper '{file}'".into()), ..settings };
        let plan = launch_plan(&game, Path::new("map.osz"), &settings, Some("osu-wine".into()), Some("wine".into())).unwrap();
        assert_eq!(plan.len(), 1); assert_eq!(plan[0].program, "wrapper");
        assert!(launch_plan(&game, Path::new("map.osz"), &Settings::default(), None, None).unwrap().is_empty());
    }
}
