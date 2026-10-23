//! Opt-in checks against the real mirrors and this machine's osu! install. They are ignored by
//! default because they use the network and a few MB of mirror bandwidth.
//!
//! Run with: cargo test --lib live_ -- --ignored --nocapture --test-threads=1

use crate::{
    downloader::{Downloader, EventSink, ProgressEvent, StateEvent},
    mirrors::{MirrorId, MirrorSet, SearchQuery, Sort},
    osu_locator,
    settings::Settings,
};
use std::sync::Arc;

const CATBOY: &str = "https://catboy.best";
const OSU_DIRECT: &str = "https://osu.direct";
/// Nothing listens here, so connections are refused straight away.
const DEAD: &str = "http://127.0.0.1:9";

/// A mirror set where only `mirror` is reachable, so a result proves that mirror works.
fn only(mirror: MirrorId) -> MirrorSet {
    match mirror {
        MirrorId::Catboy => MirrorSet::with_bases(CATBOY, DEAD),
        MirrorId::OsuDirect => MirrorSet::with_bases(DEAD, OSU_DIRECT),
    }
}

struct Silent;

impl EventSink for Silent {
    fn state(&self, _event: StateEvent) {}
    fn progress(&self, _event: ProgressEvent) {}
}

#[tokio::test]
#[ignore = "uses the network"]
async fn live_search_works_on_every_mirror() {
    for mirror in MirrorId::ALL {
        let query = SearchQuery {
            query: "freedom dive".into(),
            mode: Some(0),
            limit: 5,
            ..SearchQuery::default()
        };
        let response = only(mirror)
            .search(&[mirror], &query)
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", mirror.name()));
        assert_eq!(response.mirror, mirror);
        assert!(!response.results.is_empty(), "{} returned nothing", mirror.name());
        let first = &response.results[0];
        println!("{}: {} results, first = {} - {}", mirror.name(), response.results.len(), first.artist, first.title);
    }
}

#[tokio::test]
#[ignore = "uses the network"]
async fn live_status_and_mode_filters_are_honoured() {
    for mirror in MirrorId::ALL {
        let query = SearchQuery { status: 4, mode: Some(3), limit: 10, ..SearchQuery::default() };
        let response = only(mirror)
            .search(&[mirror], &query)
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", mirror.name()));
        assert!(!response.results.is_empty(), "{} returned nothing", mirror.name());
        for set in &response.results {
            assert_eq!(set.status, "loved", "{}: set {}", mirror.name(), set.id);
            assert!(set.beatmaps.iter().any(|b| b.mode_int == 3), "{}: set {} has no mania map", mirror.name(), set.id);
        }
        println!("{}: {} loved mania sets", mirror.name(), response.results.len());
    }
}

#[tokio::test]
#[ignore = "uses the network"]
async fn live_sorting_is_applied_by_osu_direct_and_reported_by_catboy() {
    let query = SearchQuery { sort: Sort::Plays, limit: 5, ..SearchQuery::default() };

    let sorted = only(MirrorId::OsuDirect).search(&MirrorId::ALL, &query).await.unwrap();
    assert_eq!(sorted.mirror, MirrorId::OsuDirect);
    assert!(sorted.sort_applied);
    let plays: Vec<u64> = sorted.results.iter().map(|s| s.play_count).collect();
    println!("osu.direct play counts: {plays:?}");
    assert!(plays.windows(2).all(|w| w[0] >= w[1]), "not sorted by plays: {plays:?}");

    let unsorted = only(MirrorId::Catboy).search(&MirrorId::ALL, &query).await.unwrap();
    assert_eq!(unsorted.mirror, MirrorId::Catboy);
    assert!(!unsorted.sort_applied, "catboy can't sort, so the response must say so");
}

#[tokio::test]
#[ignore = "uses the network"]
async fn live_beatmap_ids_resolve_to_their_set() {
    for mirror in MirrorId::ALL {
        // Beatmap 129891 belongs to set 39804 (checked 2026-10-22).
        let set_id = only(mirror).resolve_set_id(&[mirror], 129_891).await.unwrap_or_else(|e| panic!("{}: {e}", mirror.name()));
        assert_eq!(set_id, 39_804, "{}", mirror.name());
    }
}

#[tokio::test]
#[ignore = "downloads a few MB from each mirror"]
async fn live_download_works_on_every_mirror() {
    // Set 2549719 has a video: about 14 MB with it and 4.8 MB without (checked 2026-10-22).
    const SET_WITH_VIDEO: u32 = 2_549_719;
    for mirror in MirrorId::ALL {
        let dir = tempfile::tempdir().unwrap();
        let downloader = Downloader::new(Arc::new(only(mirror)), dir.path().to_path_buf(), Arc::new(Silent));
        let file = downloader
            .fetch(SET_WITH_VIDEO, true, &[mirror])
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", mirror.name()));
        let size = std::fs::metadata(&file).unwrap().len();
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        println!("{}: {name} ({:.1} MB)", mirror.name(), size as f64 / 1e6);
        // osu.direct sends no filename, so this also covers the metadata-based naming.
        assert!(
            name.starts_with(&format!("{SET_WITH_VIDEO} mochari - ")) && name.ends_with(".osz"),
            "{}: archive is not named '{{id}} {{artist}} - {{title}}.osz': {name:?}",
            mirror.name()
        );
        assert!(size > 1_000_000, "{} sent only {size} bytes", mirror.name());
        assert!(size < 8_000_000, "{} ignored the no-video request ({size} bytes)", mirror.name());
    }
}

#[test]
#[ignore = "needs osu! stable running on this machine"]
fn live_finds_the_running_osu_window() {
    let osu = crate::overlay::locate_osu_window();
    println!("osu! running: {}", osu.running);
    assert!(osu.running, "start osu! stable first");
    let window = osu.window.expect("osu! is running but no usable window was found");
    println!("window: {:?}", window);
    assert!(window.rect.width() > 100 && window.rect.height() > 100, "implausible size {:?}", window.rect);
    assert!(window.visible && !window.minimized && !window.owned);
}

#[test]
#[ignore = "reads osu!'s window state; sends it nothing"]
fn live_reads_osu_window_state_without_touching_it() {
    let snapshot = crate::overlay::snapshot_osu();
    println!("osu! processes: {:?}", snapshot.pids);
    for window in snapshot.windows.iter().filter(|w| w.visible) {
        println!("visible window: {window:?}");
    }
    let status = crate::handoff::assess(&snapshot);
    println!("status: {status:?}");
    assert!(
        !matches!(status, crate::handoff::OsuStatus::Blocked(_)),
        "osu! looks blocked ({status:?}): close any dialog and make sure only one osu! is running"
    );
}

/// Opens a real modal message box (owned by a form placed far off-screen, in a throwaway
/// PowerShell) and checks that the detection used before handing anything to osu! recognises
/// it. osu! itself is never involved.
#[test]
#[ignore = "opens a real, off-screen modal dialog in a throwaway PowerShell"]
fn live_a_real_modal_dialog_is_detected() {
    use crate::handoff::{Blocked, OsuStatus, assess};
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object Windows.Forms.Form
$f.Text = 'qd-owner'
$f.StartPosition = 'Manual'
$f.Location = New-Object Drawing.Point(-4000, -4000)
$f.Show()
Start-Sleep -Milliseconds 2500
[void][Windows.Forms.MessageBox]::Show($f, 'osu!flow detection test - safe to ignore')
"#;
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-Sta", "-Command", script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start powershell");
    let pid = child.id();

    let started = Instant::now();
    let (mut saw_plain_window_ready, mut saw_dialog) = (false, false);
    while started.elapsed() < Duration::from_secs(25) && !saw_dialog {
        let snapshot = crate::overlay::snapshot_for_pids(vec![pid]);
        match assess(&snapshot) {
            OsuStatus::Ready => saw_plain_window_ready = true,
            OsuStatus::Blocked(Blocked::DialogOpen) => {
                println!("detected: {:#?}", snapshot.windows.iter().filter(|w| w.visible).collect::<Vec<_>>());
                saw_dialog = true;
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(saw_plain_window_ready, "the plain form was never seen as Ready");
    assert!(saw_dialog, "a real modal dialog was not detected");
}

#[cfg(windows)]
#[test]
#[ignore = "asks Windows which browser opens links; changes nothing"]
fn live_shell_reports_this_pcs_default_browser() {
    let browser = crate::browser::shell_default_browser().expect("Windows reported no browser for https links");
    println!("default browser: {}", browser.display());
    assert!(browser.is_file(), "{} doesn't exist", browser.display());
    let me = std::env::current_exe().unwrap();
    let registry = crate::browser::Registry::system();
    assert!(!registry.is_default(&me), "this test binary shouldn't be the default browser");
    assert_eq!(registry.default_browser(&me), Some(browser));
}

/// Prints how long `work` takes on average, in milliseconds.
fn average_ms(label: &str, runs: u32, mut work: impl FnMut()) {
    let started = std::time::Instant::now();
    for _ in 0..runs {
        work();
    }
    println!("{label}: {:.2} ms (average of {runs})", started.elapsed().as_secs_f64() * 1000.0 / f64::from(runs));
}

#[test]
#[ignore = "times reads of this machine; changes nothing"]
fn live_bench_system_scans() {
    let settings = Settings::default();
    average_ms("osu_locator::detect", 20, || {
        let _ = osu_locator::detect(&settings);
    });
    average_ms("overlay::snapshot_osu (processes + windows)", 20, || {
        let _ = crate::overlay::snapshot_osu();
    });
    average_ms("overlay::locate_osu_window", 20, || {
        let _ = crate::overlay::locate_osu_window();
    });
    if let Some(install) = osu_locator::detect(&settings) {
        average_ms("library::has_folder_for (Songs listing)", 50, || {
            let _ = crate::library::has_folder_for(&install.songs_dir, 1);
        });
    }
}

/// How long the overlay's search box waits for typed queries. Every query differs, so none is
/// answered from the cache, and they go out the way the app sends them (osu.direct first).
#[tokio::test]
#[ignore = "uses the network and times searches"]
async fn live_bench_search() {
    use std::time::Instant;
    let mirrors = MirrorSet::new();
    let order = MirrorId::DEFAULT_ORDER.to_vec();
    let words = ["camellia", "yoasobi", "linked horizon", "kaf", "daft punk", "nanahira", "ado", "kenshi yonezu"];
    let mut times = Vec::new();
    for word in words {
        let query = SearchQuery { query: word.into(), limit: 30, ..SearchQuery::default() };
        let started = Instant::now();
        let response = mirrors.search(&order, &query).await;
        let elapsed = started.elapsed().as_millis();
        println!("{word:<16} {elapsed:>5} ms via {}", response.map_or("nothing (failed)", |r| r.mirror.name()));
        times.push(elapsed);
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
    times.sort_unstable();
    let mean = times.iter().sum::<u128>() / times.len() as u128;
    println!("mean {mean} ms, median {} ms, slowest {} ms", times[times.len() / 2], times[times.len() - 1]);
}

/// A stand-in for osu!: always healthy, and "imports" a set a fixed time after being launched, so
/// what a run measures is the app's own overhead rather than osu!'s speed.
struct StandInOsu {
    songs: std::path::PathBuf,
    import_after: std::time::Duration,
}

impl crate::handoff::OsuEnv for StandInOsu {
    fn install(&self, _settings: &Settings) -> Option<osu_locator::OsuInstall> {
        Some(osu_locator::OsuInstall {
            exe: "osu!.exe".into(),
            dir: self.songs.clone(),
            songs_dir: self.songs.clone(),
            exclusive_fullscreen: false,
            source: osu_locator::DetectSource::Settings,
        })
    }

    fn snapshot(&self) -> crate::overlay::OsuSnapshot {
        use crate::overlay::{OsuSnapshot, Rect, WindowCandidate};
        OsuSnapshot {
            pids: vec![1],
            windows: vec![WindowCandidate {
                handle: 1,
                rect: Rect { left: 0, top: 0, right: 2560, bottom: 1440 },
                visible: true,
                minimized: false,
                owned: false,
                enabled: true,
                hung: false,
            }],
        }
    }

    fn launch(&self, _install: &osu_locator::OsuInstall, archive: &std::path::Path) -> std::io::Result<()> {
        let folder = self.songs.join(archive.file_stem().expect("archive has a name"));
        let delay = self.import_after;
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let _ = std::fs::create_dir(folder);
        });
        Ok(())
    }
}

/// Everything between "download clicked" and "osu! has the file" that this app controls. The
/// download and the name lookup use the real mirrors; osu! is [`StandInOsu`].
#[tokio::test]
#[ignore = "uses the network and times the pipeline"]
async fn live_bench_pipeline() {
    use std::time::{Duration, Instant};
    const SET: u32 = 2611677;
    const IMPORT_AFTER: Duration = Duration::from_millis(300);

    let work = tempfile::tempdir().unwrap();
    let songs = work.path().join("Songs");
    std::fs::create_dir_all(&songs).unwrap();
    let mirrors = Arc::new(MirrorSet::new());
    let downloader = Downloader::new(mirrors.clone(), work.path().join("downloads"), Arc::new(Silent));
    let order = crate::mirrors::MirrorId::DEFAULT_ORDER.to_vec();

    let started = Instant::now();
    let _ = mirrors.lookup_set(&order, SET).await;
    println!("name lookup on its own: {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);

    for run in 1..=3 {
        let started = Instant::now();
        let archive = downloader.fetch(SET, true, &order).await.expect("download");
        let fetched = started.elapsed();

        let handoff = crate::handoff::Handoff::new(Arc::new(StandInOsu { songs: songs.clone(), import_after: IMPORT_AFTER }));
        let started = Instant::now();
        handoff.hand_off(&archive, SET, &Settings::default()).await.expect("hand-off");
        let handed = started.elapsed();

        println!(
            "run {run}: download + name {:.0} ms | hand-off {:.0} ms, of which {:.0} ms is the app (stand-in imports after {} ms)",
            fetched.as_secs_f64() * 1000.0,
            handed.as_secs_f64() * 1000.0,
            (handed.saturating_sub(IMPORT_AFTER)).as_secs_f64() * 1000.0,
            IMPORT_AFTER.as_millis(),
        );
        let _ = std::fs::remove_file(&archive);
        for entry in std::fs::read_dir(&songs).unwrap().flatten() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[test]
#[ignore = "inspects this machine"]
fn live_detects_this_machines_osu() {
    let install = osu_locator::detect(&Settings::default()).expect("osu! stable was not found");
    println!("{install:#?}");
    assert!(install.exe.is_file());
    assert!(install.songs_dir.is_dir());
}
