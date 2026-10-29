//! Hands a downloaded archive to osu! without the failure modes seen in practice.
//!
//! Launching `osu!.exe <file>.osz` works when osu! is closed, but with osu! already running it
//! can end in an "Error moving file" dialog, and while such a dialog is open osu! can't take
//! anything else - a further launch starts a second osu! that shares the same database. So:
//!
//! 1. wait until nothing else holds the archive (a virus scanner, for instance);
//! 2. look at osu! from outside and refuse to launch while it has a dialog open, isn't
//!    responding, or more than one copy is running;
//! 3. hand over one archive at a time;
//! 4. only report success once osu! has really taken the archive (it moves it into Songs, then
//!    extracts it) - otherwise say why.
//!
//! Everything touching the machine goes through [`OsuEnv`], so the logic is tested with a fake
//! instead of a real osu!.

use crate::{
    error::{AppError, Result},
    importer::{self, ImportMethod},
    library,
    osu_locator::{self, OsuInstall},
    overlay::{self, OsuSnapshot, WindowCandidate},
    settings::Settings,
};
use std::{
    fmt, io,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

/// Why osu! shouldn't be handed anything right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// Two copies share one database, and it's unclear which would get the file.
    MultipleInstances,
    NotResponding,
    /// A message box or other modal dialog is open.
    DialogOpen,
}

impl fmt::Display for Blocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Blocked::MultipleInstances => {
                "more than one osu! is running and they share one database. Close the extra one, then try again"
            }
            Blocked::NotResponding => "osu! isn't responding. Wait for it to recover or restart it, then try again",
            Blocked::DialogOpen => "osu! is showing a message or dialog. Dismiss it, then try again",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsuStatus {
    NotRunning,
    /// Running, but no window to look at yet.
    Starting,
    Ready,
    Blocked(Blocked),
}

/// A dialog is much smaller than the game; anything under 1/8 of the game's area counts.
const DIALOG_AREA_FRACTION: i64 = 8;

/// Reads osu!'s state from its windows. Only visible windows count: a healthy osu! owns several
/// hidden, disabled helper windows (input-method windows and the like).
pub fn assess(snapshot: &OsuSnapshot) -> OsuStatus {
    if snapshot.pids.is_empty() {
        return OsuStatus::NotRunning;
    }
    if snapshot.pids.len() > 1 {
        return OsuStatus::Blocked(Blocked::MultipleInstances);
    }
    if !snapshot.window_inspection { return OsuStatus::Ready; }
    let visible: Vec<&WindowCandidate> = snapshot.windows.iter().filter(|w| w.visible).collect();
    if visible.is_empty() {
        return OsuStatus::Starting;
    }
    if visible.iter().any(|w| w.hung) {
        return OsuStatus::Blocked(Blocked::NotResponding);
    }
    // The game window: the biggest visible window that isn't owned by another, preferring one
    // that isn't minimised (a minimised window reports a tiny rectangle).
    let main = visible
        .iter()
        .copied()
        .filter(|w| !w.owned)
        .max_by_key(|w| (!w.minimized, w.rect.area()));
    let Some(main) = main else {
        // Only owned windows are showing: a dialog with nothing behind it.
        return OsuStatus::Blocked(Blocked::DialogOpen);
    };
    // A modal dialog disables the window it belongs to.
    if !main.enabled {
        return OsuStatus::Blocked(Blocked::DialogOpen);
    }
    let dialog_showing = visible.iter().any(|w| {
        w.handle != main.handle
            && !w.minimized
            && (w.owned || w.rect.area().saturating_mul(DIALOG_AREA_FRACTION) < main.rect.area())
    });
    if dialog_showing { OsuStatus::Blocked(Blocked::DialogOpen) } else { OsuStatus::Ready }
}

/// Everything the hand-off needs from the machine.
pub trait OsuEnv: Send + Sync {
    fn install(&self, settings: &Settings) -> Option<OsuInstall>;
    fn snapshot(&self) -> OsuSnapshot;
    fn launch(&self, install: &OsuInstall, archive: &Path, settings: &Settings) -> io::Result<()>;
}

pub struct SystemEnv;

impl OsuEnv for SystemEnv {
    fn install(&self, settings: &Settings) -> Option<OsuInstall> {
        osu_locator::detect(settings)
    }

    fn snapshot(&self) -> OsuSnapshot {
        overlay::snapshot_osu()
    }

    fn launch(&self, install: &OsuInstall, archive: &Path, settings: &Settings) -> io::Result<()> {
        importer::launch_with_settings(install, archive, settings)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// How long to wait for something else to let go of the archive.
    pub unlock_timeout: Duration,
    pub unlock_poll: Duration,
    /// A "blocked" reading is looked at again after this, in case it was a passing state.
    pub recheck_delay: Duration,
    /// How long to wait for the set to appear when osu! was already running...
    pub verify_running: Duration,
    /// ...and when this launch had to start osu!, which takes a while.
    pub verify_cold_start: Duration,
    /// How often to look for signs that osu! took the archive. Those are single file lookups, so
    /// this can be short: osu! reacts within a fraction of a second and waiting longer than
    /// needed is time the user sits watching.
    pub verify_poll: Duration,
    /// How often to also list the whole Songs folder (hundreds of entries), the slow check.
    pub listing_every: Duration,
    /// While waiting, how often to check that osu! hasn't become blocked.
    pub busy_check_every: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            unlock_timeout: Duration::from_secs(10),
            // A virus scanner often holds a file for a moment after it is written; this is how late the
            // hand-off notices it let go, so it is checked often (a failed open costs next to nothing).
            unlock_poll: Duration::from_millis(25),
            recheck_delay: Duration::from_millis(500),
            verify_running: Duration::from_secs(30),
            verify_cold_start: Duration::from_secs(90),
            verify_poll: Duration::from_millis(40),
            listing_every: Duration::from_millis(500),
            busy_check_every: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Wait {
    Appeared,
    TimedOut,
    Blocked(Blocked),
}

pub struct Handoff {
    env: Arc<dyn OsuEnv>,
    timing: Timing,
    /// Held for a whole hand-off, so launches never overlap.
    turn: Mutex<()>,
}

/// Has osu! taken the archive? osu! moves it into Songs and extracts it there, so it is either
/// gone from where it was, sitting in Songs waiting to be extracted, or already a folder. The
/// first two are single file lookups that can run every few milliseconds; listing Songs is the
/// slow check, which only `check_listing` asks for.
fn osu_took(archive: &Path, songs_dir: &Path, set_id: u32, check_listing: bool) -> bool {
    !archive.exists()
        || archive.file_name().is_some_and(|name| songs_dir.join(name).exists())
        || (check_listing && library::has_folder_for(songs_dir, set_id))
}

fn is_sharing_violation(error: &io::Error) -> bool {
    // ERROR_SHARING_VIOLATION and ERROR_LOCK_VIOLATION.
    cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33))
}

/// Opens the file asking for nobody else to have it open. That fails while a virus scanner,
/// indexer or anything else still holds it - which is exactly when osu! couldn't move it.
fn open_exclusively(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().read(true).share_mode(0).open(path).map(drop)
    }
    #[cfg(not(windows))]
    {
        // Unix has no Windows sharing modes. Preserve missing/unreadable-path errors only.
        std::fs::metadata(path).map(|_| ())
    }
}

/// Waits until nothing else has the archive open. Only "in use" is retried; any other error
/// (a missing file, say) is reported straight away.
pub async fn wait_for_exclusive_access(path: &Path, timeout: Duration, poll: Duration) -> Result<()> {
    let started = Instant::now();
    loop {
        match open_exclusively(path) {
            Ok(()) => return Ok(()),
            Err(error) if is_sharing_violation(&error) => {
                if started.elapsed() >= timeout {
                    return Err(AppError::ArchiveLocked(path.display().to_string()));
                }
                tokio::time::sleep(poll).await;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

impl Handoff {
    pub fn new(env: Arc<dyn OsuEnv>) -> Self {
        Self::with_timing(env, Timing::default())
    }

    pub fn with_timing(env: Arc<dyn OsuEnv>, timing: Timing) -> Self {
        Self { env, timing, turn: Mutex::new(()) }
    }

    /// Runs blocking environment calls (process scans, spawning) off the async workers.
    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&dyn OsuEnv) -> T + Send + 'static,
    ) -> Result<T> {
        let env = self.env.clone();
        tokio::task::spawn_blocking(move || work(env.as_ref()))
            .await
            .map_err(|e| AppError::Task(e.to_string()))
    }

    /// osu!'s state now. A "blocked" reading is checked again after a moment, because a launch
    /// that has just finished can leave a second osu! process around briefly.
    async fn current_status(&self) -> Result<OsuStatus> {
        let first = assess(&self.blocking(|env| env.snapshot()).await?);
        if !matches!(first, OsuStatus::Blocked(_)) {
            return Ok(first);
        }
        tokio::time::sleep(self.timing.recheck_delay).await;
        Ok(assess(&self.blocking(|env| env.snapshot()).await?))
    }

    /// Hands `archive` to osu! and returns once the set is really in `songs_dir`.
    ///
    /// Failing leaves the archive where it is, so the user can try again.
    pub async fn hand_off(&self, archive: &Path, set_id: u32, settings: &Settings) -> Result<ImportMethod> {
        let _turn = self.turn.lock().await;

        wait_for_exclusive_access(archive, self.timing.unlock_timeout, self.timing.unlock_poll).await?;

        let for_env = settings.clone();
        let install = self.blocking(move |env| env.install(&for_env)).await?;
        if settings.game_client == crate::settings::GameClient::Lazer {
            // Lazer imports archives through its own native IPC/file argument. Never copy into
            // its content-addressed store or use stable's Songs/F5 fallback. Keep the archive
            // for async import/recovery; success here means the request was sent, not verified.
            let Some(install) = install.filter(|install| install.client == crate::settings::GameClient::Lazer) else { return Ok(ImportMethod::SavedForLazer); };
            let archive = archive.to_path_buf(); let settings = settings.clone();
            return Ok(match self.blocking(move |env| env.launch(&install, &archive, &settings)).await? {
                Ok(()) => ImportMethod::OpenedInLazer,
                Err(_) => ImportMethod::SavedForLazer,
            });
        }
        let Some(install) = install else {
            // No osu! to launch; drop the archive in Songs if we know where that is.
            return importer::place_in_songs(archive, settings.songs_dir.as_deref(), None);
        };

        let status = self.current_status().await?;
        if let OsuStatus::Blocked(reason) = status {
            return Err(AppError::OsuBusy(reason));
        }
        let cold_start = status != OsuStatus::Ready;

        let (launch_install, launch_archive) = (install.clone(), archive.to_path_buf());
        let launch_settings = settings.clone();
        if let Err(error) = self.blocking(move |env| env.launch(&launch_install, &launch_archive, &launch_settings)).await? {
            return importer::place_in_songs(archive, Some(&install.songs_dir), Some(error));
        }

        let limit = if cold_start { self.timing.verify_cold_start } else { self.timing.verify_running };
        match self.wait_for_set(&install.songs_dir, set_id, archive, limit).await? {
            Wait::Appeared => Ok(ImportMethod::LaunchedOsu),
            Wait::Blocked(reason) => Err(AppError::OsuBusy(reason)),
            Wait::TimedOut => Err(AppError::ImportStalled {
                seconds: limit.as_secs(),
                path: archive.display().to_string(),
            }),
        }
    }

    /// Waits for osu! to take the archive. If osu! turns out to be blocked meanwhile (seen on
    /// two checks in a row, so a passing glitch doesn't count) it stops early instead of waiting
    /// out the clock.
    async fn wait_for_set(&self, songs_dir: &Path, set_id: u32, archive: &Path, limit: Duration) -> Result<Wait> {
        let started = Instant::now();
        let mut last_busy_check = Instant::now();
        let mut last_listing: Option<Instant> = None;
        let mut seen_blocked: Option<Blocked> = None;
        loop {
            let check_listing = last_listing.is_none_or(|at| at.elapsed() >= self.timing.listing_every);
            if check_listing {
                last_listing = Some(Instant::now());
            }
            if osu_took(archive, songs_dir, set_id, check_listing) {
                return Ok(Wait::Appeared);
            }
            if started.elapsed() >= limit {
                return Ok(Wait::TimedOut);
            }
            if last_busy_check.elapsed() >= self.timing.busy_check_every {
                last_busy_check = Instant::now();
                match assess(&self.blocking(|env| env.snapshot()).await?) {
                    OsuStatus::Blocked(reason) if seen_blocked == Some(reason) => return Ok(Wait::Blocked(reason)),
                    OsuStatus::Blocked(reason) => seen_blocked = Some(reason),
                    _ => seen_blocked = None,
                }
            }
            tokio::time::sleep(self.timing.verify_poll).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osu_locator::DetectSource;
    use std::{
        collections::VecDeque,
        path::PathBuf,
        sync::Mutex as StdMutex,
    };

    // ---- snapshots ------------------------------------------------------------------------

    fn win(handle: isize, width: i32, height: i32) -> WindowCandidate {
        WindowCandidate {
            handle,
            rect: overlay::Rect { left: 0, top: 0, right: width, bottom: height },
            visible: true,
            minimized: false,
            owned: false,
            enabled: true,
            hung: false,
        }
    }

    /// What a healthy osu! looks like from outside (checked on a real install): one visible,
    /// enabled game window plus hidden, disabled helper windows.
    fn ready() -> OsuSnapshot {
        OsuSnapshot { window_inspection: true, pids: vec![10],
            windows: vec![
                win(1, 2560, 1441),
                WindowCandidate { visible: false, enabled: false, owned: true, ..win(2, 0, 0) },
                WindowCandidate { visible: false, ..win(3, 1, 1) },
            ],
        }
    }

    fn with_window(extra: WindowCandidate) -> OsuSnapshot {
        let mut snapshot = ready();
        snapshot.windows.push(extra);
        snapshot
    }

    fn dialog_open() -> OsuSnapshot {
        with_window(WindowCandidate { owned: true, ..win(9, 357, 149) })
    }

    fn not_running() -> OsuSnapshot {
        OsuSnapshot::default()
    }

    // ---- assess ---------------------------------------------------------------------------

    #[test]
    fn a_healthy_osu_is_ready_and_hidden_helper_windows_are_ignored() {
        assert_eq!(assess(&ready()), OsuStatus::Ready);
    }

    #[test]
    fn nothing_running_and_nothing_showing_yet_are_told_apart() {
        assert_eq!(assess(&not_running()), OsuStatus::NotRunning);
        let booting = OsuSnapshot { window_inspection: true, pids: vec![10], windows: vec![WindowCandidate { visible: false, ..win(1, 800, 600) }] };
        assert_eq!(assess(&booting), OsuStatus::Starting);
        assert_eq!(assess(&OsuSnapshot { window_inspection: true, pids: vec![10], windows: vec![] }), OsuStatus::Starting);
    }

    #[test]
    fn an_owned_dialog_blocks() {
        assert_eq!(assess(&dialog_open()), OsuStatus::Blocked(Blocked::DialogOpen));
    }

    #[test]
    fn a_disabled_game_window_means_a_modal_dialog_is_up() {
        let snapshot = OsuSnapshot { window_inspection: true, pids: vec![10],
            windows: vec![WindowCandidate { enabled: false, ..win(1, 2560, 1441) }],
        };
        assert_eq!(assess(&snapshot), OsuStatus::Blocked(Blocked::DialogOpen));
    }

    #[test]
    fn a_small_unowned_window_next_to_the_game_is_a_dialog() {
        assert_eq!(assess(&with_window(win(9, 357, 149))), OsuStatus::Blocked(Blocked::DialogOpen));
        // Comparable in size to the game, so it isn't taken for a dialog.
        assert_eq!(assess(&with_window(win(9, 1800, 1000))), OsuStatus::Ready);
    }

    #[test]
    fn only_owned_windows_showing_is_a_dialog() {
        let snapshot = OsuSnapshot { window_inspection: true, pids: vec![10],
            windows: vec![WindowCandidate { owned: true, ..win(9, 357, 149) }],
        };
        assert_eq!(assess(&snapshot), OsuStatus::Blocked(Blocked::DialogOpen));
    }

    #[test]
    fn a_hung_window_blocks() {
        let snapshot = OsuSnapshot { window_inspection: true, pids: vec![10], windows: vec![WindowCandidate { hung: true, ..win(1, 2560, 1441) }] };
        assert_eq!(assess(&snapshot), OsuStatus::Blocked(Blocked::NotResponding));
    }

    #[test]
    fn two_copies_of_osu_block_whatever_their_windows_look_like() {
        let mut snapshot = ready();
        snapshot.pids.push(11);
        assert_eq!(assess(&snapshot), OsuStatus::Blocked(Blocked::MultipleInstances));
    }

    #[test]
    fn a_minimised_game_window_is_not_mistaken_for_a_dialog_or_hides_one() {
        let minimised = WindowCandidate { minimized: true, ..win(1, 160, 28) };
        assert_eq!(assess(&OsuSnapshot { window_inspection: true, pids: vec![10], windows: vec![minimised] }), OsuStatus::Ready);
        let with_dialog = OsuSnapshot { window_inspection: true, pids: vec![10],
            windows: vec![minimised, WindowCandidate { owned: true, ..win(9, 357, 149) }],
        };
        assert_eq!(assess(&with_dialog), OsuStatus::Blocked(Blocked::DialogOpen));
    }

    #[test]
    fn blocked_reasons_tell_the_user_what_to_do() {
        assert!(Blocked::DialogOpen.to_string().contains("Dismiss it"));
        assert!(Blocked::MultipleInstances.to_string().contains("Close the extra one"));
        assert!(Blocked::NotResponding.to_string().contains("restart it"));
        let message = AppError::OsuBusy(Blocked::DialogOpen).to_string();
        assert!(message.starts_with("osu! can't take a map right now"), "{message}");
    }

    #[test]
    fn wine_process_only_snapshots_are_ready_without_claiming_dialog_detection() {
        let snapshot = OsuSnapshot { pids: vec![10], windows: vec![], window_inspection: false };
        assert_eq!(assess(&snapshot), OsuStatus::Ready);
        assert_eq!(assess(&OsuSnapshot { pids: vec![10, 11], ..snapshot }), OsuStatus::Blocked(Blocked::MultipleInstances));
    }

    #[test]
    fn unix_errno_values_are_not_windows_sharing_violations() {
        for code in [32, 33] { assert_eq!(is_sharing_violation(&io::Error::from_raw_os_error(code)), cfg!(windows)); }
    }

    // ---- a fake machine -------------------------------------------------------------------

    type LaunchHook = Box<dyn Fn(&Path) + Send + Sync>;

    struct FakeEnv {
        install: Option<OsuInstall>,
        /// Consumed one per reading; the last one repeats.
        snapshots: StdMutex<VecDeque<OsuSnapshot>>,
        launches: StdMutex<Vec<(PathBuf, Instant)>>,
        /// Runs when osu! is "launched", e.g. to make the set's folder appear.
        on_launch: StdMutex<Option<LaunchHook>>,
        launch_fails: bool,
    }

    impl FakeEnv {
        fn new(install: Option<OsuInstall>, snapshot: OsuSnapshot) -> Self {
            Self {
                install,
                snapshots: StdMutex::new(VecDeque::from([snapshot])),
                launches: StdMutex::default(),
                on_launch: StdMutex::default(),
                launch_fails: false,
            }
        }

        fn then_snapshot(self, snapshot: OsuSnapshot) -> Self {
            self.snapshots.lock().unwrap().push_back(snapshot);
            self
        }

        fn on_launch(self, hook: impl Fn(&Path) + Send + Sync + 'static) -> Self {
            *self.on_launch.lock().unwrap() = Some(Box::new(hook));
            self
        }

        fn launched(&self) -> Vec<(PathBuf, Instant)> {
            self.launches.lock().unwrap().clone()
        }
    }

    impl OsuEnv for FakeEnv {
        fn install(&self, _settings: &Settings) -> Option<OsuInstall> {
            self.install.clone()
        }

        fn snapshot(&self) -> OsuSnapshot {
            let mut queue = self.snapshots.lock().unwrap();
            if queue.len() > 1 { queue.pop_front().unwrap() } else { queue.front().cloned().unwrap() }
        }

        fn launch(&self, _install: &OsuInstall, archive: &Path, _settings: &Settings) -> io::Result<()> {
            if self.launch_fails {
                return Err(io::Error::new(io::ErrorKind::NotFound, "no osu!.exe"));
            }
            self.launches.lock().unwrap().push((archive.to_path_buf(), Instant::now()));
            if let Some(hook) = self.on_launch.lock().unwrap().as_ref() {
                hook(archive);
            }
            Ok(())
        }
    }

    struct World {
        dir: tempfile::TempDir,
        songs: PathBuf,
        install: OsuInstall,
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let songs = dir.path().join("Songs");
        std::fs::create_dir_all(&songs).unwrap();
        let install = OsuInstall {
            client: crate::settings::GameClient::Stable, exe: dir.path().join("osu!.exe"),
            dir: dir.path().to_path_buf(),
            songs_dir: songs.clone(),
            exclusive_fullscreen: false,
            source: DetectSource::Settings,
        };
        World { dir, songs, install }
    }

    fn archive(world: &World, set_id: u32) -> PathBuf {
        let path = world.dir.path().join(format!("{set_id} Artist - Title.osz"));
        std::fs::write(&path, b"PK").unwrap();
        path
    }

    /// Makes `{id} Imported` appear in Songs shortly after osu! is launched with `{id} ....osz`.
    fn imports_after(songs: &Path, delay: Duration) -> impl Fn(&Path) + Send + Sync + 'static {
        let songs = songs.to_path_buf();
        move |archive| {
            let name = archive.file_name().unwrap().to_string_lossy().into_owned();
            let id: String = name.chars().take_while(char::is_ascii_digit).collect();
            let folder = songs.join(format!("{id} Imported"));
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                std::fs::create_dir_all(folder).unwrap();
            });
        }
    }

    #[tokio::test]
    async fn lazer_uses_native_handoff_and_never_writes_a_songs_or_hash_store_fallback() {
        let mut world = world(); world.install.client = crate::settings::GameClient::Lazer;
        let archive = archive(&world, 12);
        let settings = Settings { game_client: crate::settings::GameClient::Lazer, songs_dir: Some(world.songs.clone()), ..Settings::default() };
        let env = Arc::new(FakeEnv::new(Some(world.install.clone()), not_running()));
        assert_eq!(Handoff::with_timing(env.clone(), quick()).hand_off(&archive, 12, &settings).await.unwrap(), ImportMethod::OpenedInLazer);
        assert_eq!(env.launched().len(), 1); assert!(archive.exists()); assert_eq!(std::fs::read_dir(&world.songs).unwrap().count(), 0);
        let mut failed = FakeEnv::new(Some(world.install), not_running()); failed.launch_fails = true;
        for env in [Arc::new(failed), Arc::new(FakeEnv::new(None, not_running()))] {
            assert_eq!(Handoff::with_timing(env, quick()).hand_off(&archive, 12, &settings).await.unwrap(), ImportMethod::SavedForLazer);
            assert!(archive.exists()); assert_eq!(std::fs::read_dir(&world.songs).unwrap().count(), 0);
        }
    }

    fn quick() -> Timing {
        Timing {
            unlock_timeout: Duration::from_millis(300),
            unlock_poll: Duration::from_millis(20),
            recheck_delay: Duration::from_millis(20),
            verify_running: Duration::from_millis(600),
            verify_cold_start: Duration::from_millis(1500),
            verify_poll: Duration::from_millis(20),
            listing_every: Duration::from_millis(20),
            busy_check_every: Duration::from_millis(40),
        }
    }

    /// Like `quick`, but listing Songs is rare, so only the cheap lookups can notice an import.
    fn quick_without_listings() -> Timing {
        Timing { listing_every: Duration::from_secs(60), ..quick() }
    }

    fn handoff(env: &Arc<FakeEnv>) -> Handoff {
        Handoff::with_timing(env.clone(), quick())
    }

    // ---- the hand-off ---------------------------------------------------------------------

    #[tokio::test]
    async fn succeeds_only_once_the_set_has_appeared_in_songs() {
        let world = world();
        let archive = archive(&world, 77);
        let env = Arc::new(
            FakeEnv::new(Some(world.install.clone()), ready())
                .on_launch(imports_after(&world.songs, Duration::from_millis(150))),
        );
        let started = Instant::now();
        let method = handoff(&env).hand_off(&archive, 77, &Settings::default()).await.unwrap();
        assert_eq!(method, ImportMethod::LaunchedOsu);
        assert!(started.elapsed() >= Duration::from_millis(100), "returned before the set existed");
        assert_eq!(env.launched().len(), 1);
        assert_eq!(env.launched()[0].0, archive);
    }

    /// Moves the archive into Songs (as osu! does), `after` a delay, without extracting it yet.
    fn moves_archive_into_songs_after(songs: &Path, delay: Duration) -> impl Fn(&Path) + Send + Sync + 'static {
        let songs = songs.to_path_buf();
        move |archive| {
            let (from, to) = (archive.to_path_buf(), songs.join(archive.file_name().unwrap()));
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                std::fs::rename(from, to).unwrap();
            });
        }
    }

    #[tokio::test]
    async fn the_archive_leaving_its_folder_counts_as_osu_taking_it_without_any_listing() {
        let world = world();
        let archive = archive(&world, 31);
        let env = Arc::new(
            FakeEnv::new(Some(world.install.clone()), ready())
                .on_launch(moves_archive_into_songs_after(&world.songs, Duration::from_millis(60))),
        );
        let started = Instant::now();
        let handoff = Handoff::with_timing(env.clone(), quick_without_listings());
        let method = handoff.hand_off(&archive, 31, &Settings::default()).await.unwrap();
        assert_eq!(method, ImportMethod::LaunchedOsu);
        assert!(started.elapsed() < Duration::from_millis(400), "took {:?}", started.elapsed());
        assert!(!archive.exists());
    }

    #[tokio::test]
    async fn osu_taking_the_archive_is_seen_within_a_few_polls_not_a_second_later() {
        let world = world();
        let archive = archive(&world, 32);
        let taken = Arc::new(StdMutex::new(None::<Instant>));
        let record = taken.clone(); let songs = world.songs.clone();
        let env = Arc::new(
            FakeEnv::new(Some(world.install.clone()), ready())
                .on_launch(move |archive| {
                    let source = archive.to_path_buf(); let target = songs.join(source.file_name().unwrap()); let record = record.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(100));
                        let mut taken = record.lock().unwrap(); std::fs::rename(source, target).unwrap(); *taken = Some(Instant::now());
                    });
                }),
        );
        let timing = Timing { verify_poll: Duration::from_millis(10), ..quick_without_listings() };
        Handoff::with_timing(env, timing).hand_off(&archive, 32, &Settings::default()).await.unwrap();
        // Measure the actual move; a loaded CI runner may schedule its thread after 100 ms.
        let late = taken.lock().unwrap().unwrap().elapsed();
        assert!(late < Duration::from_millis(80), "noticed {late:?} after osu! had it");
    }

    #[test]
    fn the_three_signs_that_osu_took_the_archive() {
        let world = world();
        let present = archive(&world, 40);
        // Nothing has happened: the archive is where it was and Songs has nothing for the set.
        assert!(!osu_took(&present, &world.songs, 40, true));
        // A folder for the set only counts when the listing is asked for.
        std::fs::create_dir_all(world.songs.join("40 Whatever")).unwrap();
        assert!(!osu_took(&present, &world.songs, 40, false));
        assert!(osu_took(&present, &world.songs, 40, true));
        // The archive sitting in Songs counts straight away...
        let other = archive(&world, 41);
        std::fs::copy(&other, world.songs.join(other.file_name().unwrap())).unwrap();
        assert!(osu_took(&other, &world.songs, 41, false));
        // ...and so does the archive having left its folder.
        let gone = archive(&world, 42);
        std::fs::remove_file(&gone).unwrap();
        assert!(osu_took(&gone, &world.songs, 42, false));
    }

    #[tokio::test]
    async fn reports_a_stalled_import_when_the_set_never_appears() {
        let world = world();
        let archive = archive(&world, 5);
        let env = Arc::new(FakeEnv::new(Some(world.install.clone()), ready()));
        let error = handoff(&env).hand_off(&archive, 5, &Settings::default()).await.unwrap_err();
        match &error {
            AppError::ImportStalled { path, .. } => assert!(path.contains("5 Artist - Title.osz"), "{path}"),
            other => panic!("expected ImportStalled, got {other}"),
        }
        assert!(error.to_string().contains("deal with that first"), "{error}");
        assert!(archive.exists(), "the download is kept so the user can retry");
        assert_eq!(env.launched().len(), 1);
    }

    #[tokio::test]
    async fn will_not_launch_while_a_dialog_is_open() {
        let world = world();
        let archive = archive(&world, 5);
        let env = Arc::new(FakeEnv::new(Some(world.install.clone()), dialog_open()));
        let error = handoff(&env).hand_off(&archive, 5, &Settings::default()).await.unwrap_err();
        assert!(matches!(error, AppError::OsuBusy(Blocked::DialogOpen)), "{error}");
        assert!(env.launched().is_empty(), "osu! must not be launched while it is blocked");
    }

    #[tokio::test]
    async fn will_not_launch_with_a_disabled_game_window_a_hung_osu_or_two_copies() {
        let world = world();
        let archive = archive(&world, 5);
        let disabled = OsuSnapshot { window_inspection: true, pids: vec![10], windows: vec![WindowCandidate { enabled: false, ..win(1, 2560, 1441) }] };
        let hung = OsuSnapshot { window_inspection: true, pids: vec![10], windows: vec![WindowCandidate { hung: true, ..win(1, 2560, 1441) }] };
        let mut two = ready();
        two.pids.push(11);
        for (snapshot, expected) in [
            (disabled, Blocked::DialogOpen),
            (hung, Blocked::NotResponding),
            (two, Blocked::MultipleInstances),
        ] {
            let env = Arc::new(FakeEnv::new(Some(world.install.clone()), snapshot));
            let error = handoff(&env).hand_off(&archive, 5, &Settings::default()).await.unwrap_err();
            assert!(matches!(error, AppError::OsuBusy(reason) if reason == expected), "{expected:?}: {error}");
            assert!(env.launched().is_empty(), "{expected:?}");
        }
    }

    #[tokio::test]
    async fn a_passing_block_that_clears_does_not_stop_the_hand_off() {
        let world = world();
        let archive = archive(&world, 8);
        // A second process is visible for one reading (a previous launch still exiting), then gone.
        let mut transient = ready();
        transient.pids.push(11);
        let env = Arc::new(
            FakeEnv::new(Some(world.install.clone()), transient)
                .then_snapshot(ready())
                .on_launch(imports_after(&world.songs, Duration::from_millis(50))),
        );
        let method = handoff(&env).hand_off(&archive, 8, &Settings::default()).await.unwrap();
        assert_eq!(method, ImportMethod::LaunchedOsu);
    }

    #[tokio::test]
    async fn a_dialog_that_appears_while_waiting_ends_the_wait_early() {
        let world = world();
        let archive = archive(&world, 6);
        // Healthy at the pre-launch check, then a dialog from the moment osu! is launched.
        let env = Arc::new(FakeEnv::new(Some(world.install.clone()), ready()).then_snapshot(dialog_open()));
        let slow = Timing { verify_running: Duration::from_secs(10), ..quick() };
        let started = Instant::now();
        let error = Handoff::with_timing(env.clone(), slow).hand_off(&archive, 6, &Settings::default()).await.unwrap_err();
        assert!(matches!(error, AppError::OsuBusy(Blocked::DialogOpen)), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3), "waited the full {:?}", started.elapsed());
        assert_eq!(env.launched().len(), 1);
    }

    #[tokio::test]
    async fn a_cold_start_gets_a_longer_wait_than_a_running_osu() {
        let slow_import = Duration::from_millis(900);

        // osu! already running: the 600 ms allowance runs out first.
        let world_a = world();
        let archive_a = archive(&world_a, 1);
        let running = Arc::new(
            FakeEnv::new(Some(world_a.install.clone()), ready()).on_launch(imports_after(&world_a.songs, slow_import)),
        );
        let error = handoff(&running).hand_off(&archive_a, 1, &Settings::default()).await.unwrap_err();
        assert!(matches!(error, AppError::ImportStalled { .. }), "{error}");

        // osu! not running: starting it takes longer, so the same import is waited for.
        let world_b = world();
        let archive_b = archive(&world_b, 1);
        let cold = Arc::new(
            FakeEnv::new(Some(world_b.install.clone()), not_running()).on_launch(imports_after(&world_b.songs, slow_import)),
        );
        assert_eq!(handoff(&cold).hand_off(&archive_b, 1, &Settings::default()).await.unwrap(), ImportMethod::LaunchedOsu);
    }

    #[tokio::test]
    async fn only_one_hand_off_runs_at_a_time() {
        let world = world();
        let (a, b) = (archive(&world, 1), archive(&world, 2));
        let env = Arc::new(
            FakeEnv::new(Some(world.install.clone()), ready())
                .on_launch(imports_after(&world.songs, Duration::from_millis(250))),
        );
        let handoff = handoff(&env);
        let settings = Settings::default();
        let (first, second) = tokio::join!(handoff.hand_off(&a, 1, &settings), handoff.hand_off(&b, 2, &settings));
        assert!(first.is_ok() && second.is_ok(), "{first:?} / {second:?}");
        let launches = env.launched();
        assert_eq!(launches.len(), 2);
        let gap = launches[1].1.duration_since(launches[0].1);
        assert!(gap >= Duration::from_millis(200), "second launch came only {gap:?} after the first");
    }

    #[tokio::test]
    async fn without_an_osu_install_the_archive_goes_to_the_songs_folder() {
        let world = world();
        let archive = archive(&world, 3);
        let env = Arc::new(FakeEnv::new(None, not_running()));
        let settings = Settings { songs_dir: Some(world.songs.clone()), ..Settings::default() };
        let method = handoff(&env).hand_off(&archive, 3, &settings).await.unwrap();
        assert_eq!(method, ImportMethod::MovedToSongs);
        assert!(world.songs.join("3 Artist - Title.osz").is_file());
        assert!(env.launched().is_empty());

        let archive = self::archive(&world, 4);
        let error = handoff(&env).hand_off(&archive, 4, &Settings::default()).await.unwrap_err();
        assert!(matches!(error, AppError::OsuNotFound), "{error}");
    }

    #[tokio::test]
    async fn a_launch_that_fails_falls_back_to_the_songs_folder() {
        let world = world();
        let archive = archive(&world, 3);
        let mut fake = FakeEnv::new(Some(world.install.clone()), not_running());
        fake.launch_fails = true;
        let env = Arc::new(fake);
        let method = handoff(&env).hand_off(&archive, 3, &Settings::default()).await.unwrap();
        assert_eq!(method, ImportMethod::MovedToSongs);
        assert!(world.songs.join("3 Artist - Title.osz").is_file());
    }

    #[tokio::test]
    async fn a_missing_archive_is_reported_at_once_and_nothing_is_launched() {
        let world = world();
        let env = Arc::new(FakeEnv::new(Some(world.install.clone()), ready()));
        let missing = world.dir.path().join("gone.osz");
        let started = Instant::now();
        let error = handoff(&env).hand_off(&missing, 1, &Settings::default()).await.unwrap_err();
        assert!(matches!(error, AppError::Io(_)), "{error}");
        assert!(started.elapsed() < Duration::from_millis(250));
        assert!(env.launched().is_empty());
    }

    // ---- waiting for the archive to be free (Windows sharing rules) --------------------------

    #[cfg(windows)]
    #[tokio::test]
    async fn waits_while_something_else_has_the_archive_open_then_carries_on() {
        let world = world();
        let archive = archive(&world, 9);
        let holder = std::fs::File::open(&archive).unwrap(); // e.g. a virus scanner
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            drop(holder);
        });
        let started = Instant::now();
        wait_for_exclusive_access(&archive, Duration::from_secs(5), Duration::from_millis(20)).await.unwrap();
        assert!(started.elapsed() >= Duration::from_millis(200), "didn't wait: {:?}", started.elapsed());
        release.join().unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn gives_up_with_a_clear_error_if_the_archive_stays_locked() {
        let world = world();
        let archive = archive(&world, 9);
        let _holder = std::fs::File::open(&archive).unwrap();
        let env = Arc::new(FakeEnv::new(Some(world.install.clone()), ready()));
        let error = handoff(&env).hand_off(&archive, 9, &Settings::default()).await.unwrap_err();
        assert!(matches!(error, AppError::ArchiveLocked(_)), "{error}");
        assert!(error.to_string().contains("Try again in a moment"), "{error}");
        assert!(env.launched().is_empty(), "must not hand over a file that is still locked");
    }

    #[tokio::test]
    async fn a_free_archive_is_available_immediately() {
        let world = world();
        let archive = archive(&world, 9);
        let started = Instant::now();
        wait_for_exclusive_access(&archive, Duration::from_secs(5), Duration::from_millis(20)).await.unwrap();
        assert!(started.elapsed() < Duration::from_millis(100));
    }
}
