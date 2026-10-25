//! Beatmapset / beatmap types in the osu! API v2 shape. catboy.best and osu.direct both return
//! this shape (checked live 2026-10-22), so one model serves every mirror. Only fields the UI
//! uses are kept; unknown fields are dropped and `null` is read as the field's default.

use serde::{Deserialize, Deserializer, Serialize};

/// Mirrors occasionally send `null` where the osu! API sends a value.
fn null_default<'de, D, T>(de: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(de)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Covers {
    #[serde(deserialize_with = "null_default")]
    pub cover: String,
    #[serde(rename = "cover@2x", deserialize_with = "null_default")]
    pub cover_2x: String,
    #[serde(deserialize_with = "null_default")]
    pub card: String,
    #[serde(rename = "card@2x", deserialize_with = "null_default")]
    pub card_2x: String,
    #[serde(deserialize_with = "null_default")]
    pub list: String,
    #[serde(rename = "list@2x", deserialize_with = "null_default")]
    pub list_2x: String,
    #[serde(deserialize_with = "null_default")]
    pub slimcover: String,
    #[serde(rename = "slimcover@2x", deserialize_with = "null_default")]
    pub slimcover_2x: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Beatmap {
    #[serde(deserialize_with = "null_default")]
    pub id: u32,
    #[serde(deserialize_with = "null_default")]
    pub beatmapset_id: u32,
    #[serde(deserialize_with = "null_default")]
    pub version: String,
    #[serde(deserialize_with = "null_default")]
    pub mode: String,
    #[serde(deserialize_with = "null_default")]
    pub mode_int: u8,
    #[serde(deserialize_with = "null_default")]
    pub difficulty_rating: f64,
    #[serde(deserialize_with = "null_default")]
    pub total_length: u32,
    #[serde(deserialize_with = "null_default")]
    pub hit_length: u32,
    #[serde(deserialize_with = "null_default")]
    pub bpm: f64,
    #[serde(deserialize_with = "null_default")]
    pub cs: f64,
    #[serde(deserialize_with = "null_default")]
    pub ar: f64,
    #[serde(deserialize_with = "null_default")]
    pub accuracy: f64,
    #[serde(deserialize_with = "null_default")]
    pub drain: f64,
    #[serde(deserialize_with = "null_default")]
    pub count_circles: u32,
    #[serde(deserialize_with = "null_default")]
    pub count_sliders: u32,
    #[serde(deserialize_with = "null_default")]
    pub count_spinners: u32,
    pub max_combo: Option<u32>,
    #[serde(deserialize_with = "null_default")]
    pub convert: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Beatmapset {
    #[serde(deserialize_with = "null_default")]
    pub id: u32,
    #[serde(deserialize_with = "null_default")]
    pub title: String,
    #[serde(deserialize_with = "null_default")]
    pub title_unicode: String,
    #[serde(deserialize_with = "null_default")]
    pub artist: String,
    #[serde(deserialize_with = "null_default")]
    pub artist_unicode: String,
    #[serde(deserialize_with = "null_default")]
    pub creator: String,
    #[serde(deserialize_with = "null_default")]
    pub source: String,
    /// "ranked", "loved", "pending", ... as the osu! API names it.
    #[serde(deserialize_with = "null_default")]
    pub status: String,
    #[serde(deserialize_with = "null_default")]
    pub ranked: i32,
    #[serde(deserialize_with = "null_default")]
    pub covers: Covers,
    #[serde(deserialize_with = "null_default")]
    pub play_count: u64,
    #[serde(deserialize_with = "null_default")]
    pub favourite_count: u64,
    #[serde(deserialize_with = "null_default")]
    pub video: bool,
    #[serde(deserialize_with = "null_default")]
    pub storyboard: bool,
    #[serde(deserialize_with = "null_default")]
    pub nsfw: bool,
    /// osu!'s genre id (2 video game, 3 anime, 4 rock, 5 pop, 10 electronic, 11 metal, ...); 0 if not given.
    #[serde(deserialize_with = "null_default")]
    pub genre_id: u8,
    /// osu!'s language id (2 English, 3 Japanese, 5 instrumental, ...); 0 if not given.
    #[serde(deserialize_with = "null_default")]
    pub language_id: u8,
    /// Set when the song is a Featured Artist track (checked against the osu! website's own chips).
    pub track_id: Option<u64>,
    #[serde(deserialize_with = "null_default")]
    pub spotlight: bool,
    #[serde(deserialize_with = "null_default")]
    pub bpm: f64,
    pub preview_url: Option<String>,
    pub last_updated: Option<String>,
    pub ranked_date: Option<String>,
    pub submitted_date: Option<String>,
    #[serde(deserialize_with = "null_default")]
    pub beatmaps: Vec<Beatmap>,
}

/// `/api/v2/b/{id}` returns a beatmap; only its parent set matters for downloading.
#[derive(Debug, Deserialize)]
pub struct BeatmapLookup {
    pub beatmapset_id: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATBOY: &str = include_str!("../tests/fixtures/catboy_search.json");
    const OSU_DIRECT: &str = include_str!("../tests/fixtures/osudirect_search.json");
    const CATBOY_PENDING: &str = include_str!("../tests/fixtures/catboy_pending.json");

    fn assert_usable(sets: &[Beatmapset]) {
        assert!(!sets.is_empty());
        for set in sets {
            assert!(set.id > 0, "id missing");
            assert!(!set.title.is_empty(), "title missing for {}", set.id);
            assert!(!set.artist.is_empty(), "artist missing for {}", set.id);
            assert!(!set.creator.is_empty(), "creator missing for {}", set.id);
            assert!(!set.status.is_empty(), "status missing for {}", set.id);
            assert!(!set.covers.card.is_empty(), "card cover missing for {}", set.id);
            assert!(!set.beatmaps.is_empty(), "no beatmaps for {}", set.id);
            assert!(set.beatmaps.iter().all(|b| b.beatmapset_id == set.id));
            assert!(set.beatmaps.iter().all(|b| b.difficulty_rating > 0.0));
        }
    }

    #[test]
    fn parses_real_catboy_response() {
        let sets: Vec<Beatmapset> = serde_json::from_str(CATBOY).unwrap();
        assert_usable(&sets);
    }

    #[test]
    fn parses_real_osu_direct_response() {
        let sets: Vec<Beatmapset> = serde_json::from_str(OSU_DIRECT).unwrap();
        assert_usable(&sets);
    }

    #[test]
    fn pending_sets_have_no_ranked_date() {
        let sets: Vec<Beatmapset> = serde_json::from_str(CATBOY_PENDING).unwrap();
        assert!(!sets.is_empty());
        assert!(sets.iter().all(|s| s.status == "pending"));
        assert!(sets.iter().any(|s| s.ranked_date.is_none()));
    }

    #[test]
    fn null_and_missing_fields_fall_back_to_defaults() {
        let set: Beatmapset =
            serde_json::from_str(r#"{"id":5,"title":"T","source":null,"bpm":null,"covers":null}"#)
                .unwrap();
        assert_eq!(set.id, 5);
        assert_eq!(set.source, "");
        assert_eq!(set.bpm, 0.0);
        assert_eq!(set.covers, Covers::default());
        assert!(set.beatmaps.is_empty());
    }

    #[test]
    fn genre_language_and_featured_artist_survive_from_both_mirrors() {
        for (name, body) in [("catboy.best", CATBOY), ("osu.direct", OSU_DIRECT)] {
            let sets: Vec<Beatmapset> = serde_json::from_str(body).unwrap();
            assert!(sets.iter().any(|s| s.genre_id > 0), "{name}: no genre came through");
            assert!(sets.iter().any(|s| s.language_id > 0), "{name}: no language came through");
        }
        let json = serde_json::to_value(serde_json::from_str::<Beatmapset>(
            r#"{"id":1,"genre_id":11,"language_id":3,"track_id":9784,"spotlight":true}"#,
        )
        .unwrap())
        .unwrap();
        // The webview reads them under the osu! API's own names.
        assert_eq!((json["genre_id"].as_u64(), json["language_id"].as_u64()), (Some(11), Some(3)));
        assert_eq!((json["track_id"].as_u64(), json["spotlight"].as_bool()), (Some(9784), Some(true)));
    }

    #[test]
    fn the_filter_fields_default_when_missing_or_null() {
        let set: Beatmapset = serde_json::from_str(r#"{"id":5,"genre_id":null,"track_id":null}"#).unwrap();
        assert_eq!((set.genre_id, set.language_id, set.track_id, set.spotlight), (0, 0, None, false));
    }

    #[test]
    fn serialises_cover_keys_with_at_2x_suffix() {
        let sets: Vec<Beatmapset> = serde_json::from_str(CATBOY).unwrap();
        let json = serde_json::to_value(&sets[0]).unwrap();
        assert!(json["covers"]["card@2x"].as_str().is_some_and(|s| !s.is_empty()));
    }

    #[test]
    fn beatmap_lookup_reads_set_id() {
        let lookup: BeatmapLookup =
            serde_json::from_str(r#"{"id":129891,"beatmapset_id":39804,"version":"x"}"#).unwrap();
        assert_eq!(lookup.beatmapset_id, 39804);
    }
}
