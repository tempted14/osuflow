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
#[ignore = "inspects this machine"]
fn live_detects_this_machines_osu() {
    let install = osu_locator::detect(&Settings::default()).expect("osu! stable was not found");
    println!("{install:#?}");
    assert!(install.exe.is_file());
    assert!(install.songs_dir.is_dir());
}
