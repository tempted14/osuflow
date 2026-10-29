//! Hands a downloaded .osz to osu! stable. The file is opened the way Explorer opens it on a
//! double-click or "Open with" (through Windows' .osz association, which runs `osu!.exe "<file>"`
//! or passes the file to the osu! that is already running), so nothing is injected or patched.
//! If the association doesn't lead to this osu!, `osu!.exe` is started with the file directly.
//! If that fails, or osu! can't be found, the file is dropped into the Songs folder instead and
//! osu! picks it up on its next refresh.

use crate::{
    error::{AppError, Result},
    osu_locator::OsuInstall,
};
use serde::Serialize;
use std::{
    path::Path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportMethod {
    OpenedInLazer,
    SavedForLazer,
    /// osu! was started (or, if already running, was handed the file).
    LaunchedOsu,
    /// The archive was placed in the Songs folder.
    MovedToSongs,
}

/// Drops the archive into the Songs folder, for when osu! can't be launched or found.
/// `launch_error` is what went wrong launching it, reported if there is nowhere to put the file.
pub fn place_in_songs(
    osz: &Path,
    songs: Option<&Path>,
    launch_error: Option<std::io::Error>,
) -> Result<ImportMethod> {
    match songs.filter(|dir| dir.is_dir()) {
        Some(songs) => move_to_songs(osz, songs).map(|()| ImportMethod::MovedToSongs),
        None => Err(launch_error.map_or(AppError::OsuNotFound, AppError::Io)),
    }
}

/// Opens the archive with osu! the way Explorer does for a double-click or "Open with": through
/// Windows' own .osz file association, whenever that leads to this osu!. Otherwise (another
/// program owns .osz, or the shell refuses) `osu!.exe` is started with the file directly.
#[cfg(windows)]
pub fn launch_osu(install: &OsuInstall, osz: &Path) -> std::io::Result<()> {
    // The shell picks the program from the file's own extension, so ask about that one.
    let extension = osz.extension().map(|extension| format!(".{}", extension.to_string_lossy()));
    let associated = extension.as_deref().and_then(shell::associated_exe);
    if association_leads_to(associated.as_deref(), &install.exe) && shell::open(osz).is_ok() {
        return Ok(());
    }
    launch_exe(install, osz)
}

/// Whether the program Windows opens .osz files with is this osu! (and not lazer, say).
#[cfg(windows)]
fn association_leads_to(associated: Option<&Path>, osu_exe: &Path) -> bool {
    associated.is_some_and(|program| crate::browser::same_exe(program, osu_exe))
}

#[cfg(windows)]
fn launch_exe(install: &OsuInstall, osz: &Path) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    // osu! expects to start in its own folder. The child is deliberately not waited on.
    Command::new(&install.exe)
        .arg(osz)
        .current_dir(&install.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(windows)]
mod shell {
    use std::{
        ffi::OsStr,
        io,
        os::windows::ffi::OsStrExt,
        path::{Path, PathBuf},
    };
    use windows::{
        Win32::UI::{
            Shell::{ASSOCF_NONE, ASSOCSTR_EXECUTABLE, AssocQueryStringW, ShellExecuteW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
        core::{PCWSTR, PWSTR, w},
    };

    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// The program Windows opens files with this extension (".osz") with, if there is one.
    pub fn associated_exe(extension: &str) -> Option<PathBuf> {
        let extension = wide(OsStr::new(extension));
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        // SAFETY: `extension` is NUL-terminated, `buffer` outlives the call and `length` is its
        // size in UTF-16 units.
        let result = unsafe {
            AssocQueryStringW(
                ASSOCF_NONE,
                ASSOCSTR_EXECUTABLE,
                PCWSTR(extension.as_ptr()),
                w!("open"),
                Some(PWSTR(buffer.as_mut_ptr())),
                &mut length,
            )
        };
        if result.is_err() {
            return None;
        }
        let text = String::from_utf16_lossy(&buffer[..(length as usize).min(buffer.len())]);
        let text = text.trim_end_matches('\0');
        (!text.is_empty()).then(|| PathBuf::from(text))
    }

    /// Opens `file` with its associated program, as Explorer does on a double-click: same verb,
    /// and the file's own folder as the working directory.
    pub fn open(file: &Path) -> io::Result<()> {
        let file_name = wide(file.as_os_str());
        let folder = file.parent().map(|parent| wide(parent.as_os_str()));
        let folder = folder.as_ref().map_or(PCWSTR::null(), |folder| PCWSTR(folder.as_ptr()));
        // SAFETY: both strings are NUL-terminated and outlive the call.
        let result = unsafe {
            ShellExecuteW(None, w!("open"), PCWSTR(file_name.as_ptr()), PCWSTR::null(), folder, SW_SHOWNORMAL)
        };
        // Values above 32 mean the shell started the program; anything else is an error code.
        let code = result.0 as isize;
        if code > 32 { Ok(()) } else { Err(io::Error::other(format!("the shell could not open the file (code {code})"))) }
    }
}

#[cfg(all(unix, test))]
pub fn launch_osu(install: &OsuInstall, osz: &Path) -> std::io::Result<()> {
    crate::wine::launch(install, osz, &crate::settings::Settings::default())
}

pub fn launch_with_settings(install: &OsuInstall, osz: &Path, settings: &crate::settings::Settings) -> std::io::Result<()> {
    if settings.game_client == crate::settings::GameClient::Lazer { return crate::lazer::launch(install, osz, settings); }
    #[cfg(windows)] { let _ = settings; launch_osu(install, osz) }
    #[cfg(unix)] { crate::wine::launch(install, osz, settings) }
}

fn move_to_songs(osz: &Path, songs: &Path) -> Result<()> {
    let name = osz
        .file_name()
        .ok_or_else(|| AppError::InvalidArchive("download has no file name".into()))?;
    let target = songs.join(name);
    // Rename fails across drives, so fall back to copy + delete.
    if std::fs::rename(osz, &target).is_err() {
        std::fs::copy(osz, &target)?;
        std::fs::remove_file(osz)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osu_locator::DetectSource;
    use std::path::PathBuf;

    fn osz_in(dir: &Path) -> PathBuf {
        let path = dir.join("123 Artist - Title.osz");
        std::fs::write(&path, b"PK").unwrap();
        path
    }

    #[test]
    fn without_osu_the_archive_goes_to_the_songs_folder() {
        let downloads = tempfile::tempdir().unwrap();
        let songs = tempfile::tempdir().unwrap();
        let osz = osz_in(downloads.path());
        let method = place_in_songs(&osz, Some(songs.path()), None).unwrap();
        assert_eq!(method, ImportMethod::MovedToSongs);
        assert!(songs.path().join("123 Artist - Title.osz").is_file());
        assert!(!osz.exists());
    }

    #[test]
    fn nowhere_to_put_it_is_osu_not_found() {
        let downloads = tempfile::tempdir().unwrap();
        let osz = osz_in(downloads.path());
        assert!(matches!(place_in_songs(&osz, None, None), Err(AppError::OsuNotFound)));
        let missing = downloads.path().join("no-such-songs-dir");
        assert!(matches!(place_in_songs(&osz, Some(&missing), None), Err(AppError::OsuNotFound)));
        assert!(osz.exists(), "the download is kept when nothing could take it");
    }

    #[test]
    fn a_launch_failure_falls_back_to_the_songs_folder() {
        let downloads = tempfile::tempdir().unwrap();
        let songs = tempfile::tempdir().unwrap();
        let osz = osz_in(downloads.path());
        let install = OsuInstall {
            client: crate::settings::GameClient::Stable, exe: downloads.path().join("not-osu.exe"),
            dir: downloads.path().to_path_buf(),
            songs_dir: songs.path().to_path_buf(),
            exclusive_fullscreen: false,
            source: DetectSource::Settings,
        };
        let error = launch_osu(&install, &osz).unwrap_err();
        let method = place_in_songs(&osz, Some(songs.path()), Some(error)).unwrap();
        assert_eq!(method, ImportMethod::MovedToSongs);
    }

    #[test]
    fn a_launch_failure_with_no_fallback_reports_the_io_error() {
        let downloads = tempfile::tempdir().unwrap();
        let osz = osz_in(downloads.path());
        let install = OsuInstall {
            client: crate::settings::GameClient::Stable, exe: downloads.path().join("not-osu.exe"),
            dir: downloads.path().to_path_buf(),
            songs_dir: downloads.path().join("nope"),
            exclusive_fullscreen: false,
            source: DetectSource::Settings,
        };
        let error = launch_osu(&install, &osz).unwrap_err();
        assert!(matches!(place_in_songs(&osz, None, Some(error)), Err(AppError::Io(_))));
    }

    #[cfg(windows)]
    #[test]
    fn only_an_association_that_leads_to_this_osu_counts() {
        let exe = Path::new(r"C:\osu!\osu!.exe");
        assert!(association_leads_to(Some(Path::new(r"c:\OSU!\osu!.EXE")), exe));
        assert!(!association_leads_to(Some(Path::new(r"C:\osu-lazer\osu!.exe")), exe));
        assert!(!association_leads_to(None, exe));
    }

    /// A throwaway file type, registered for the current user only and removed again on drop,
    /// whose "open" command is a script. It lets the real shell route run without any real program.
    #[cfg(windows)]
    struct TestAssociation {
        extension: String,
        prog_id: String,
    }

    #[cfg(windows)]
    impl TestAssociation {
        fn register(script: &Path) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            use winreg::{RegKey, enums::HKEY_CURRENT_USER};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let id = format!("{}x{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst));
            let (extension, prog_id) = (format!(".qdtest{id}"), format!("OsuFlowTest{id}"));
            let user = RegKey::predef(HKEY_CURRENT_USER);
            let (ext_key, _) = user.create_subkey(format!(r"Software\Classes\{extension}")).unwrap();
            ext_key.set_value("", &prog_id).unwrap();
            let (command, _) = user.create_subkey(format!(r"Software\Classes\{prog_id}\shell\open\command")).unwrap();
            command.set_value("", &format!(r#""{}" "%1""#, script.display())).unwrap();
            Self { extension, prog_id }
        }
    }

    #[cfg(windows)]
    impl Drop for TestAssociation {
        fn drop(&mut self) {
            use winreg::{RegKey, enums::HKEY_CURRENT_USER};
            let user = RegKey::predef(HKEY_CURRENT_USER);
            let _ = user.delete_subkey_all(format!(r"Software\Classes\{}", self.extension));
            let _ = user.delete_subkey_all(format!(r"Software\Classes\{}", self.prog_id));
        }
    }

    /// A stand-in program that appends the file it was given and its working directory to `log`.
    #[cfg(windows)]
    fn recorder(dir: &Path, name: &str, log: &str) -> PathBuf {
        let script = dir.join(name);
        let body = format!("@echo off\r\n>>\"%~dp0{log}\" echo \"%~1\"\r\n>>\"%~dp0{log}\" echo \"%CD%\"\r\n");
        std::fs::write(&script, body).unwrap();
        script
    }

    #[cfg(windows)]
    fn recorded(log: &Path) -> Vec<String> {
        let started = std::time::Instant::now();
        loop {
            let lines: Vec<String> = std::fs::read_to_string(log)
                .unwrap_or_default()
                .lines()
                .map(|line| line.trim().trim_matches('"').to_string())
                .filter(|line| !line.is_empty())
                .collect();
            if lines.len() >= 2 {
                return lines;
            }
            assert!(started.elapsed() < std::time::Duration::from_secs(15), "the program never ran");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    #[cfg(windows)]
    fn install_at(exe: &Path, dir: &Path, songs: &Path) -> OsuInstall {
        OsuInstall {
            client: crate::settings::GameClient::Stable, exe: exe.to_path_buf(),
            dir: dir.to_path_buf(),
            songs_dir: songs.to_path_buf(),
            exclusive_fullscreen: false,
            source: DetectSource::Settings,
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_file_type_that_opens_this_osu_is_opened_through_the_shell() {
        use crate::browser::same_exe;
        let work = tempfile::tempdir().unwrap();
        let osu_dir = tempfile::tempdir().unwrap();
        let songs = tempfile::tempdir().unwrap();
        let osu = recorder(osu_dir.path(), "osu-stand-in.cmd", "calls.txt");
        let association = TestAssociation::register(&osu);
        let archive = work.path().join(format!("123 Artist & Title (Cut Ver.){}", association.extension));
        std::fs::write(&archive, b"PK").unwrap();

        // Checked first, so a registry hiccup fails here instead of showing Windows' "which app?" box.
        let associated = shell::associated_exe(&association.extension).expect("the shell can't see the test association");
        assert!(same_exe(&associated, &osu), "associated with {associated:?}");

        // osu!'s folder is claimed to be somewhere else, so a direct start would run there.
        let claimed = tempfile::tempdir().unwrap();
        launch_osu(&install_at(&osu, claimed.path(), songs.path()), &archive).unwrap();
        let seen = recorded(&osu_dir.path().join("calls.txt"));
        assert!(same_exe(Path::new(&seen[0]), &archive), "got {seen:?}");
        // Explorer runs the program in the file's own folder; a direct start would have used osu!'s.
        assert!(same_exe(Path::new(&seen[1]), work.path()), "started directly, in {:?}", seen[1]);
    }

    #[cfg(windows)]
    #[test]
    fn a_file_type_that_opens_something_else_starts_osu_directly() {
        use crate::browser::same_exe;
        let work = tempfile::tempdir().unwrap();
        let osu_dir = tempfile::tempdir().unwrap();
        let other_dir = tempfile::tempdir().unwrap();
        let songs = tempfile::tempdir().unwrap();
        let osu = recorder(osu_dir.path(), "osu-stand-in.cmd", "calls.txt");
        let other = recorder(other_dir.path(), "other-program.cmd", "never.txt");
        let association = TestAssociation::register(&other);
        let archive = work.path().join(format!("123 Artist - Title{}", association.extension));
        std::fs::write(&archive, b"PK").unwrap();
        assert!(shell::associated_exe(&association.extension).is_some(), "the shell can't see the test association");

        launch_osu(&install_at(&osu, osu_dir.path(), songs.path()), &archive).unwrap();
        let seen = recorded(&osu_dir.path().join("calls.txt"));
        assert!(same_exe(Path::new(&seen[0]), &archive), "got {seen:?}");
        assert!(same_exe(Path::new(&seen[1]), osu_dir.path()), "worked in {:?}", seen[1]);
        assert!(!other_dir.path().join("never.txt").exists(), "the other program must not be run");
    }
}
