//! Beatmap mirrors: URL building, search/lookup with ordered fallback, and mirror health.
//!
//! Behaviour below was verified against the live services on 2026-10-22:
//! - catboy.best: `GET /api/v2/search?q&mode&status&limit&offset` (ignores `sort`);
//!   `GET /d/{id}` and `/d/{id}n` (no video) download the archive.
//! - osu.direct: `GET /api/v2/search?q&mode&status&amount&offset&sort=<field>:<asc|desc>`
//!   (max 100 per page); `GET /api/d/{id}[?noVideo=true]` redirects to a short-lived signed URL.
//! - Both serve `/api/v2/s/{setId}` and `/api/v2/b/{beatmapId}` in osu! API v2 shape.
//!
//! Neither accepts `status=any`, and omitting `status` gives different defaults per mirror, so it
//! is always sent explicitly.

use crate::{
    error::{AppError, Result},
    models::{BeatmapLookup, Beatmapset},
};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::HashMap,
    fmt,
    future::Future,
    sync::Mutex,
    time::{Duration, Instant},
};

pub const USER_AGENT: &str = concat!("osu!flow/", env!("CARGO_PKG_VERSION"));
/// osu.direct rejects anything above 100 results per page.
pub const MAX_PAGE: u32 = 100;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(6);
const UNHEALTHY_FOR: Duration = Duration::from_secs(300);
const CACHE_TTL: Duration = Duration::from_secs(60);
const CACHE_CAP: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MirrorId {
    Catboy,
    OsuDirect,
}

impl MirrorId {
    pub const ALL: [MirrorId; 2] = [MirrorId::Catboy, MirrorId::OsuDirect];

    /// osu.direct ranks title and artist matches far better (checked 2026-10-22), so it goes
    /// first. It caps downloads at 200 per day; catboy.best has the higher rate limit and takes
    /// over automatically when osu.direct refuses.
    pub const DEFAULT_ORDER: [MirrorId; 2] = [MirrorId::OsuDirect, MirrorId::Catboy];

    /// Mirrors in the order downloads should try them. catboy.best sends the same file about
    /// three times faster (measured 2026-10-23: 0.4 s against 1.1-1.3 s, because osu.direct
    /// spends nearly a second building the archive before the first byte) and has no daily cap,
    /// so it goes first. Search keeps its own order, which prefers osu.direct for its ranking and
    /// sorting. An order the user chose themselves is respected.
    pub fn for_downloads(preferred: &[MirrorId]) -> Vec<MirrorId> {
        if preferred == Self::DEFAULT_ORDER {
            Self::ALL.to_vec()
        } else {
            preferred.to_vec()
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            MirrorId::Catboy => "catboy.best",
            MirrorId::OsuDirect => "osu.direct",
        }
    }

    /// catboy.best silently ignores `sort`, so only osu.direct can honour a non-default order.
    pub fn supports_sort(self) -> bool {
        self == MirrorId::OsuDirect
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sort {
    #[default]
    Relevance,
    RankedDate,
    Plays,
    Favourites,
    Updated,
    Title,
    Artist,
    Difficulty,
}

impl Sort {
    /// osu.direct sorts with Meilisearch `<field>:<direction>` expressions.
    fn osu_direct_expr(self) -> Option<&'static str> {
        match self {
            Sort::Relevance => None,
            Sort::RankedDate => Some("ranked_date:desc"),
            Sort::Plays => Some("play_count:desc"),
            Sort::Favourites => Some("favourite_count:desc"),
            Sort::Updated => Some("last_updated:desc"),
            Sort::Title => Some("title:asc"),
            Sort::Artist => Some("artist:asc"),
            Sort::Difficulty => Some("beatmaps.difficulty_rating:desc"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SearchQuery {
    pub query: String,
    /// 0 osu!, 1 taiko, 2 catch, 3 mania; `None` for every mode.
    pub mode: Option<u8>,
    /// osu! API status: -2 graveyard, -1 wip, 0 pending, 1 ranked, 2 approved, 3 qualified, 4 loved.
    pub status: i8,
    pub sort: Sort,
    pub offset: u32,
    pub limit: u32,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            mode: None,
            status: 1,
            sort: Sort::Relevance,
            offset: 0,
            limit: 50,
        }
    }
}

impl SearchQuery {
    /// Clamps anything a caller could send that the mirrors would reject.
    pub fn normalized(&self) -> SearchQuery {
        SearchQuery {
            query: self.query.trim().to_string(),
            mode: self.mode.filter(|m| *m <= 3),
            status: if (-2..=4).contains(&self.status) { self.status } else { 1 },
            sort: self.sort,
            offset: self.offset,
            limit: self.limit.clamp(1, MAX_PAGE),
        }
    }

    fn cache_key(&self) -> String {
        format!(
            "{}\u{1f}{:?}\u{1f}{}\u{1f}{:?}\u{1f}{}\u{1f}{}",
            self.query.to_lowercase(),
            self.mode,
            self.status,
            self.sort,
            self.offset,
            self.limit
        )
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<Beatmapset>,
    pub mirror: MirrorId,
    /// False when the chosen sort couldn't be honoured because only catboy.best answered.
    pub sort_applied: bool,
    pub has_more: bool,
}

#[derive(Debug)]
enum MirrorError {
    Status(StatusCode),
    Network(String),
    Parse(String),
}

impl fmt::Display for MirrorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MirrorError::Status(status) => write!(f, "HTTP {status}"),
            MirrorError::Network(message) => write!(f, "network error: {message}"),
            MirrorError::Parse(message) => write!(f, "unreadable response: {message}"),
        }
    }
}

impl MirrorError {
    /// Worth sidelining the mirror for a while? A 404 only means this mirror lacks the item.
    fn is_transient(&self) -> bool {
        match self {
            MirrorError::Status(status) => {
                status.is_server_error() || matches!(status.as_u16(), 403 | 408 | 429)
            }
            MirrorError::Network(_) | MirrorError::Parse(_) => true,
        }
    }
}

struct CacheEntry {
    at: Instant,
    response: SearchResponse,
}

pub struct MirrorSet {
    client: Client,
    bases: HashMap<MirrorId, String>,
    unhealthy_until: Mutex<HashMap<MirrorId, Instant>>,
    cache: Mutex<HashMap<String, CacheEntry>>,
}

impl Default for MirrorSet {
    fn default() -> Self {
        Self::new()
    }
}

impl MirrorSet {
    pub fn new() -> Self {
        Self::with_bases("https://catboy.best", "https://osu.direct")
    }

    /// Lets tests point the mirrors at local mock servers.
    pub fn with_bases(catboy: &str, osu_direct: &str) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(4))
            .timeout(SEARCH_TIMEOUT)
            .build()
            .expect("failed to build the HTTP client");
        let bases = HashMap::from([
            (MirrorId::Catboy, catboy.trim_end_matches('/').to_string()),
            (MirrorId::OsuDirect, osu_direct.trim_end_matches('/').to_string()),
        ]);
        Self {
            client,
            bases,
            unhealthy_until: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn url(&self, mirror: MirrorId, path: &str) -> Url {
        Url::parse(&format!("{}{path}", self.bases[&mirror])).expect("mirror base URL is valid")
    }

    /// The mirror's home address, used to open a connection ahead of the first download.
    pub fn base_url(&self, mirror: MirrorId) -> Url {
        self.url(mirror, "/")
    }

    pub fn search_url(&self, mirror: MirrorId, query: &SearchQuery) -> Url {
        let mut url = self.url(mirror, "/api/v2/search");
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", &query.query);
            if let Some(mode) = query.mode {
                pairs.append_pair("mode", &mode.to_string());
            }
            pairs.append_pair("status", &query.status.to_string());
            let page_param = match mirror {
                MirrorId::Catboy => "limit",
                MirrorId::OsuDirect => "amount",
            };
            pairs.append_pair(page_param, &query.limit.to_string());
            pairs.append_pair("offset", &query.offset.to_string());
            if mirror.supports_sort()
                && let Some(expr) = query.sort.osu_direct_expr()
            {
                pairs.append_pair("sort", expr);
            }
        }
        url
    }

    pub fn download_url(&self, mirror: MirrorId, set_id: u32, no_video: bool) -> Url {
        match mirror {
            MirrorId::Catboy => {
                let suffix = if no_video { "n" } else { "" };
                self.url(mirror, &format!("/d/{set_id}{suffix}"))
            }
            MirrorId::OsuDirect => {
                let mut url = self.url(mirror, &format!("/api/d/{set_id}"));
                if no_video {
                    url.query_pairs_mut().append_pair("noVideo", "true");
                }
                url
            }
        }
    }

    fn set_url(&self, mirror: MirrorId, set_id: u32) -> Url {
        self.url(mirror, &format!("/api/v2/s/{set_id}"))
    }

    fn beatmap_url(&self, mirror: MirrorId, beatmap_id: u32) -> Url {
        self.url(mirror, &format!("/api/v2/b/{beatmap_id}"))
    }

    /// Mirrors in the order they should be tried: healthy before sidelined, and - when a sort
    /// is wanted - mirrors that honour it first. Sidelined mirrors stay in as a last resort.
    pub fn candidates(&self, preferred: &[MirrorId], want_sort: bool) -> Vec<MirrorId> {
        let mut order: Vec<MirrorId> = Vec::new();
        for mirror in preferred.iter().copied().chain(MirrorId::ALL) {
            if !order.contains(&mirror) {
                order.push(mirror);
            }
        }
        let now = Instant::now();
        let (mut usable, sidelined): (Vec<_>, Vec<_>) = {
            let health = self.unhealthy_until.lock().unwrap();
            order
                .into_iter()
                .partition(|m| health.get(m).is_none_or(|until| *until <= now))
        };
        if want_sort {
            usable.sort_by_key(|m| !m.supports_sort());
        }
        usable.extend(sidelined);
        usable
    }

    pub fn mark_unhealthy(&self, mirror: MirrorId) {
        self.unhealthy_until
            .lock()
            .unwrap()
            .insert(mirror, Instant::now() + UNHEALTHY_FOR);
    }

    pub fn mark_healthy(&self, mirror: MirrorId) {
        self.unhealthy_until.lock().unwrap().remove(&mirror);
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> std::result::Result<T, MirrorError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| MirrorError::Network(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(MirrorError::Status(status));
        }
        let body = response
            .bytes()
            .await
            .map_err(|e| MirrorError::Network(e.to_string()))?;
        serde_json::from_slice(&body).map_err(|e| MirrorError::Parse(e.to_string()))
    }

    /// Runs `op` against each mirror in turn until one succeeds.
    async fn first_ok<T, F, Fut>(&self, order: Vec<MirrorId>, mut op: F) -> Result<(MirrorId, T)>
    where
        F: FnMut(MirrorId) -> Fut,
        Fut: Future<Output = std::result::Result<T, MirrorError>>,
    {
        let mut failures = Vec::new();
        for mirror in order {
            match op(mirror).await {
                Ok(value) => {
                    self.mark_healthy(mirror);
                    return Ok((mirror, value));
                }
                Err(error) => {
                    if error.is_transient() {
                        self.mark_unhealthy(mirror);
                    }
                    failures.push(format!("{}: {error}", mirror.name()));
                }
            }
        }
        Err(AppError::AllMirrorsFailed(failures.join("; ")))
    }

    pub async fn search(&self, preferred: &[MirrorId], query: &SearchQuery) -> Result<SearchResponse> {
        let query = query.normalized();
        let key = query.cache_key();
        if let Some(hit) = self.cache_get(&key) {
            return Ok(hit);
        }
        let order = self.candidates(preferred, query.sort != Sort::Relevance);
        let (mirror, results) = self
            .first_ok(order, |m| {
                self.get_json::<Vec<Beatmapset>>(self.search_url(m, &query))
            })
            .await?;
        let response = SearchResponse {
            has_more: results.len() as u32 >= query.limit,
            sort_applied: query.sort == Sort::Relevance || mirror.supports_sort(),
            results,
            mirror,
        };
        self.cache_put(key, response.clone());
        Ok(response)
    }

    /// Beatmap id (as seen in `/b/{id}` links) to its parent beatmapset id.
    pub async fn resolve_set_id(&self, preferred: &[MirrorId], beatmap_id: u32) -> Result<u32> {
        let order = self.candidates(preferred, false);
        let (_, lookup) = self
            .first_ok(order, |m| {
                self.get_json::<BeatmapLookup>(self.beatmap_url(m, beatmap_id))
            })
            .await?;
        Ok(lookup.beatmapset_id)
    }

    pub async fn lookup_set(&self, preferred: &[MirrorId], set_id: u32) -> Result<Beatmapset> {
        let order = self.candidates(preferred, false);
        let (_, set) = self
            .first_ok(order, |m| self.get_json::<Beatmapset>(self.set_url(m, set_id)))
            .await?;
        Ok(set)
    }

    fn cache_get(&self, key: &str) -> Option<SearchResponse> {
        let cache = self.cache.lock().unwrap();
        cache
            .get(key)
            .filter(|entry| entry.at.elapsed() < CACHE_TTL)
            .map(|entry| entry.response.clone())
    }

    fn cache_put(&self, key: String, response: SearchResponse) {
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_CAP {
            cache.retain(|_, entry| entry.at.elapsed() < CACHE_TTL);
        }
        if cache.len() >= CACHE_CAP {
            let oldest = cache.iter().min_by_key(|(_, e)| e.at).map(|(k, _)| k.clone());
            if let Some(oldest) = oldest {
                cache.remove(&oldest);
            }
        }
        cache.insert(key, CacheEntry { at: Instant::now(), response });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    const CATBOY_JSON: &str = include_str!("../tests/fixtures/catboy_search.json");
    const OSU_DIRECT_JSON: &str = include_str!("../tests/fixtures/osudirect_search.json");

    fn json(body: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), "application/json")
    }

    fn query(text: &str) -> SearchQuery {
        SearchQuery { query: text.into(), ..SearchQuery::default() }
    }

    async fn mount_search(server: &MockServer, response: ResponseTemplate, expected_calls: u64) {
        Mock::given(method("GET"))
            .and(path("/api/v2/search"))
            .respond_with(response)
            .expect(expected_calls)
            .mount(server)
            .await;
    }

    #[test]
    fn search_urls_match_each_mirrors_parameter_names() {
        let set = MirrorSet::with_bases("http://c.test", "http://o.test/");
        let q = SearchQuery {
            query: "freedom dive".into(),
            mode: Some(0),
            status: 1,
            sort: Sort::Plays,
            offset: 50,
            limit: 25,
        };
        assert_eq!(
            set.search_url(MirrorId::Catboy, &q).as_str(),
            "http://c.test/api/v2/search?q=freedom+dive&mode=0&status=1&limit=25&offset=50"
        );
        assert_eq!(
            set.search_url(MirrorId::OsuDirect, &q).as_str(),
            "http://o.test/api/v2/search?q=freedom+dive&mode=0&status=1&amount=25&offset=50&sort=play_count%3Adesc"
        );
    }

    #[test]
    fn search_url_omits_mode_for_all_modes_and_sort_for_relevance() {
        let set = MirrorSet::with_bases("http://c.test", "http://o.test");
        let url = set.search_url(MirrorId::OsuDirect, &query("x"));
        assert_eq!(url.as_str(), "http://o.test/api/v2/search?q=x&status=1&amount=50&offset=0");
    }

    #[test]
    fn downloads_try_the_faster_mirror_first_unless_the_user_chose_an_order() {
        // The stored default is meant for search; downloads go to the faster mirror.
        assert_eq!(MirrorId::for_downloads(&MirrorId::DEFAULT_ORDER), vec![MirrorId::Catboy, MirrorId::OsuDirect]);
        // Anything else was a choice and is kept as it is.
        assert_eq!(MirrorId::for_downloads(&[MirrorId::Catboy, MirrorId::OsuDirect]), vec![MirrorId::Catboy, MirrorId::OsuDirect]);
        assert_eq!(MirrorId::for_downloads(&[MirrorId::OsuDirect]), vec![MirrorId::OsuDirect]);
    }

    #[test]
    fn base_urls_point_at_each_mirrors_home() {
        let mirrors = MirrorSet::with_bases("http://c.example", "http://o.example/");
        assert_eq!(mirrors.base_url(MirrorId::Catboy).as_str(), "http://c.example/");
        assert_eq!(mirrors.base_url(MirrorId::OsuDirect).as_str(), "http://o.example/");
    }

    #[test]
    fn download_urls_use_each_mirrors_no_video_form() {
        let set = MirrorSet::with_bases("http://c.test", "http://o.test");
        assert_eq!(set.download_url(MirrorId::Catboy, 123, false).as_str(), "http://c.test/d/123");
        assert_eq!(set.download_url(MirrorId::Catboy, 123, true).as_str(), "http://c.test/d/123n");
        assert_eq!(
            set.download_url(MirrorId::OsuDirect, 123, false).as_str(),
            "http://o.test/api/d/123"
        );
        assert_eq!(
            set.download_url(MirrorId::OsuDirect, 123, true).as_str(),
            "http://o.test/api/d/123?noVideo=true"
        );
    }

    #[test]
    fn normalized_clamps_what_mirrors_would_reject() {
        let q = SearchQuery { query: "  hi ".into(), mode: Some(9), status: 42, limit: 500, ..SearchQuery::default() }
            .normalized();
        assert_eq!((q.query.as_str(), q.mode, q.status, q.limit), ("hi", None, 1, MAX_PAGE));
        assert_eq!(SearchQuery { limit: 0, ..SearchQuery::default() }.normalized().limit, 1);
    }

    #[test]
    fn candidates_order_by_preference_health_and_sort_support() {
        let set = MirrorSet::with_bases("http://c.test", "http://o.test");
        assert_eq!(
            set.candidates(&[MirrorId::Catboy, MirrorId::OsuDirect], false),
            [MirrorId::Catboy, MirrorId::OsuDirect]
        );
        assert_eq!(
            set.candidates(&[MirrorId::Catboy, MirrorId::OsuDirect], true),
            [MirrorId::OsuDirect, MirrorId::Catboy]
        );
        assert_eq!(set.candidates(&[MirrorId::OsuDirect], false), [MirrorId::OsuDirect, MirrorId::Catboy]);
        set.mark_unhealthy(MirrorId::OsuDirect);
        assert_eq!(
            set.candidates(&[MirrorId::OsuDirect, MirrorId::Catboy], false),
            [MirrorId::Catboy, MirrorId::OsuDirect]
        );
        // With the only sort-capable mirror sidelined, a healthy mirror still goes first.
        assert_eq!(set.candidates(&MirrorId::ALL, true), [MirrorId::Catboy, MirrorId::OsuDirect]);
        set.mark_healthy(MirrorId::OsuDirect);
        assert_eq!(set.candidates(&MirrorId::ALL, true), [MirrorId::OsuDirect, MirrorId::Catboy]);
    }

    #[tokio::test]
    async fn falls_back_to_the_next_mirror_and_sidelines_the_broken_one() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        mount_search(&catboy, ResponseTemplate::new(500), 1).await;
        mount_search(&osu, json(OSU_DIRECT_JSON), 2).await;
        let set = MirrorSet::with_bases(&catboy.uri(), &osu.uri());

        let first = set.search(&MirrorId::ALL, &query("one")).await.unwrap();
        assert_eq!(first.mirror, MirrorId::OsuDirect);
        assert!(!first.results.is_empty());
        // catboy is sidelined now, so a different query must not touch it again (expect(1) above).
        let second = set.search(&MirrorId::ALL, &query("two")).await.unwrap();
        assert_eq!(second.mirror, MirrorId::OsuDirect);
    }

    #[tokio::test]
    async fn unreadable_json_counts_as_a_failed_mirror() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        mount_search(&catboy, json("<html>Cloudflare says no</html>"), 1).await;
        mount_search(&osu, json(OSU_DIRECT_JSON), 1).await;
        let set = MirrorSet::with_bases(&catboy.uri(), &osu.uri());
        let response = set.search(&MirrorId::ALL, &query("x")).await.unwrap();
        assert_eq!(response.mirror, MirrorId::OsuDirect);
    }

    #[tokio::test]
    async fn every_mirror_failing_reports_each_reason() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        mount_search(&catboy, ResponseTemplate::new(503), 1).await;
        mount_search(&osu, ResponseTemplate::new(429), 1).await;
        let set = MirrorSet::with_bases(&catboy.uri(), &osu.uri());
        let error = set.search(&MirrorId::ALL, &query("x")).await.unwrap_err().to_string();
        assert!(error.contains("catboy.best: HTTP 503"), "{error}");
        assert!(error.contains("osu.direct: HTTP 429"), "{error}");
    }

    #[tokio::test]
    async fn sorted_searches_go_to_osu_direct_first_and_apply_the_sort() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        mount_search(&catboy, json(CATBOY_JSON), 0).await;
        Mock::given(method("GET"))
            .and(path("/api/v2/search"))
            .and(query_param("sort", "play_count:desc"))
            .respond_with(json(OSU_DIRECT_JSON))
            .expect(1)
            .mount(&osu)
            .await;
        let set = MirrorSet::with_bases(&catboy.uri(), &osu.uri());
        let q = SearchQuery { sort: Sort::Plays, ..query("x") };
        let response = set.search(&MirrorId::ALL, &q).await.unwrap();
        assert_eq!(response.mirror, MirrorId::OsuDirect);
        assert!(response.sort_applied);
    }

    #[tokio::test]
    async fn reports_when_the_sort_could_not_be_applied() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        mount_search(&catboy, json(CATBOY_JSON), 1).await;
        mount_search(&osu, ResponseTemplate::new(500), 1).await;
        let set = MirrorSet::with_bases(&catboy.uri(), &osu.uri());
        let q = SearchQuery { sort: Sort::Plays, ..query("x") };
        let response = set.search(&MirrorId::ALL, &q).await.unwrap();
        assert_eq!(response.mirror, MirrorId::Catboy);
        assert!(!response.sort_applied);
    }

    #[tokio::test]
    async fn repeated_searches_are_served_from_the_cache() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        mount_search(&catboy, json(CATBOY_JSON), 1).await;
        let set = MirrorSet::with_bases(&catboy.uri(), &osu.uri());
        let a = set.search(&MirrorId::ALL, &query("Freedom Dive")).await.unwrap();
        let b = set.search(&MirrorId::ALL, &query("  freedom dive ")).await.unwrap();
        assert_eq!(a.results.len(), b.results.len());
    }

    #[tokio::test]
    async fn resolves_a_beatmap_id_to_its_set() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/b/129891"))
            .respond_with(json(r#"{"id":129891,"beatmapset_id":39804}"#))
            .expect(1)
            .mount(&catboy)
            .await;
        let set = MirrorSet::with_bases(&catboy.uri(), &osu.uri());
        assert_eq!(set.resolve_set_id(&MirrorId::ALL, 129891).await.unwrap(), 39804);
    }

    #[tokio::test]
    async fn a_missing_item_does_not_sideline_the_mirror() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        let set = MirrorSet::with_bases(&catboy.uri(), &osu.uri());
        // Unmocked paths answer 404.
        assert!(set.lookup_set(&MirrorId::ALL, 999_999_999).await.is_err());
        assert_eq!(set.candidates(&MirrorId::ALL, false), [MirrorId::Catboy, MirrorId::OsuDirect]);
    }

    #[tokio::test]
    async fn looks_up_a_set_by_id() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/s/39804"))
            .respond_with(json(r#"{"id":39804,"title":"FREEDOM DiVE","artist":"xi"}"#))
            .mount(&catboy)
            .await;
        let set = MirrorSet::with_bases(&catboy.uri(), &osu.uri());
        let found = set.lookup_set(&MirrorId::ALL, 39804).await.unwrap();
        assert_eq!((found.id, found.title.as_str()), (39804, "FREEDOM DiVE"));
    }
}
