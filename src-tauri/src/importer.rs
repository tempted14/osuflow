//! Hands a downloaded .osz to osu! stable. Launching `osu!.exe "<file>.osz"` is exactly what
//! double-clicking an .osz does, so nothing is injected or patched. If that fails, or osu!
//! can't be found, the file is dropped into the Songs folder instead and osu! picks it up on
//! its next refresh.

use crate::{
    error::{AppError, Result},
    osu_locator::OsuInstall,
};
use serde::Serialize;
use std::{
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportMethod {
    /// osu! was started (or, if already running, was handed the file).
    LaunchedOsu,
    /// The archive was placed in the Songs folder.
    MovedToSongs,
}

pub fn import(osz: &Path, install: Option<&OsuInstall>, songs_fallback: Option<&Path>) -> Result<ImportMethod> {
    let launch_error = match install {
        Some(install) => match launch_osu(install, osz) {
            Ok(()) => return Ok(ImportMethod::LaunchedOsu),
            Err(error) => Some(error),
        },
        None => None,
    };
    match songs_fallback.filter(|dir| dir.is_dir()) {
        Some(songs) => move_to_songs(osz, songs).map(|()| ImportMethod::MovedToSongs),
        None => Err(launch_error.map_or(AppError::OsuNotFound, AppError::Io)),
    }
}

fn launch_osu(install: &OsuInstall, osz: &Path) -> std::io::Result<()> {
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
        let method = import(&osz, None, Some(songs.path())).unwrap();
        assert_eq!(method, ImportMethod::MovedToSongs);
        assert!(songs.path().join("123 Artist - Title.osz").is_file());
        assert!(!osz.exists());
    }

    #[test]
    fn nowhere_to_put_it_is_osu_not_found() {
        let downloads = tempfile::tempdir().unwrap();
        let osz = osz_in(downloads.path());
        assert!(matches!(import(&osz, None, None), Err(AppError::OsuNotFound)));
        let missing = downloads.path().join("no-such-songs-dir");
        assert!(matches!(import(&osz, None, Some(&missing)), Err(AppError::OsuNotFound)));
        assert!(osz.exists(), "the download is kept when nothing could take it");
    }

    #[test]
    fn a_launch_failure_falls_back_to_the_songs_folder() {
        let downloads = tempfile::tempdir().unwrap();
        let songs = tempfile::tempdir().unwrap();
        let osz = osz_in(downloads.path());
        let install = OsuInstall {
            exe: downloads.path().join("not-osu.exe"),
            dir: downloads.path().to_path_buf(),
            songs_dir: songs.path().to_path_buf(),
            exclusive_fullscreen: false,
            source: DetectSource::Settings,
        };
        let method = import(&osz, Some(&install), Some(songs.path())).unwrap();
        assert_eq!(method, ImportMethod::MovedToSongs);
    }

    #[test]
    fn a_launch_failure_with_no_fallback_reports_the_io_error() {
        let downloads = tempfile::tempdir().unwrap();
        let osz = osz_in(downloads.path());
        let install = OsuInstall {
            exe: downloads.path().join("not-osu.exe"),
            dir: downloads.path().to_path_buf(),
            songs_dir: downloads.path().join("nope"),
            exclusive_fullscreen: false,
            source: DetectSource::Settings,
        };
        assert!(matches!(import(&osz, Some(&install), None), Err(AppError::Io(_))));
    }
}
