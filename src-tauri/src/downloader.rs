//! Streams a beatmapset archive from the mirrors into a download folder: ordered fallback,
//! cancellation, progress events and an integrity check. Importing into osu! is a separate step.

use crate::{
    error::{AppError, Result},
    mirrors::{MirrorId, MirrorSet, USER_AGENT},
    models::Beatmapset,
};
use futures_util::StreamExt;
use percent_encoding::percent_decode_str;
use reqwest::{Client, Url, header, redirect};
use serde::Serialize;
use std::{
    collections::HashMap,
    io::SeekFrom,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::Semaphore,
};
use tokio_util::sync::CancellationToken;

const MAX_CONCURRENT: usize = 3;
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
/// The metadata lookup that names an archive is cosmetic, so it never holds a download up long.
const NAME_LOOKUP_TIMEOUT: Duration = Duration::from_secs(4);
/// Zip comments can be up to 64 KiB, so the end-of-archive marker is searched for this far back.
const TAIL_BYTES: u64 = 65_557;
/// Smallest possible zip with one entry: local header + end-of-archive record.
const MIN_ARCHIVE_BYTES: u64 = 26;
const LOCAL_HEADER: &[u8] = b"PK\x03\x04";
const END_OF_ARCHIVE: &[u8] = b"PK\x05\x06";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadState {
    Queued,
    Downloading,
    Importing,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateEvent {
    pub set_id: u32,
    pub state: DownloadState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub set_id: u32,
    pub received: u64,
    pub total: Option<u64>,
    pub mirror: &'static str,
}

/// The artist and title a caller already knows, so the archive can be named without asking a
/// mirror. The overlay has them from the search result; a clicked link looked them up for its toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameHint {
    pub artist: String,
    pub title: String,
}

impl NameHint {
    /// `None` unless there is something to name the archive with.
    pub fn from_parts(artist: Option<String>, title: Option<String>) -> Option<Self> {
        let hint = Self { artist: artist.unwrap_or_default(), title: title.unwrap_or_default() };
        (!hint.artist.trim().is_empty() || !hint.title.trim().is_empty()).then_some(hint)
    }

    fn archive_name(&self, set_id: u32) -> Option<String> {
        archive_name(set_id, &self.artist, &self.title)
    }
}

/// Where download events go. The app forwards them to the webview; tests record them.
pub trait EventSink: Send + Sync {
    fn state(&self, event: StateEvent);
    fn progress(&self, event: ProgressEvent);
}

pub struct Downloader {
    client: Client,
    mirrors: Arc<MirrorSet>,
    dir: PathBuf,
    permits: Semaphore,
    active: Mutex<HashMap<u32, CancellationToken>>,
    sink: Arc<dyn EventSink>,
    name_lookup_timeout: Duration,
}

/// Removes the set from the active map however `fetch` ends.
struct ActiveGuard<'a> {
    downloader: &'a Downloader,
    set_id: u32,
}

impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        self.downloader.active.lock().unwrap().remove(&self.set_id);
    }
}

impl Downloader {
    pub fn new(mirrors: Arc<MirrorSet>, dir: PathBuf, sink: Arc<dyn EventSink>) -> Self {
        // Archives can be large, so there is no overall timeout - only a stall timeout.
        let client = Client::builder()
            .user_agent(USER_AGENT)
            // Redirects are followed by hand (see `get`) to read the filename they announce.
            .redirect(redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build the HTTP client");
        Self {
            client,
            mirrors,
            dir,
            permits: Semaphore::new(MAX_CONCURRENT),
            active: Mutex::new(HashMap::new()),
            sink,
            name_lookup_timeout: NAME_LOOKUP_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_name_lookup_timeout(mut self, timeout: Duration) -> Self {
        self.name_lookup_timeout = timeout;
        self
    }

    pub fn emit_state(&self, set_id: u32, state: DownloadState, error: Option<String>) {
        self.sink.state(StateEvent { set_id, state, error });
    }

    /// Cancels a queued or running download. Returns false if there wasn't one.
    pub fn cancel(&self, set_id: u32) -> bool {
        match self.active.lock().unwrap().get(&set_id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    fn register(&self, set_id: u32) -> Result<(CancellationToken, ActiveGuard<'_>)> {
        let mut active = self.active.lock().unwrap();
        if active.contains_key(&set_id) {
            return Err(AppError::AlreadyRunning(set_id));
        }
        let token = CancellationToken::new();
        active.insert(set_id, token.clone());
        Ok((token, ActiveGuard { downloader: self, set_id }))
    }

    /// Opens a connection to the mirror downloads will try first, so the first download after a
    /// quiet spell doesn't pay for the TLS handshake. Failure doesn't matter and is ignored.
    pub fn warm_up(&self, preferred: &[MirrorId]) {
        let Some(mirror) = self.mirrors.candidates(preferred, false).first().copied() else { return };
        let (client, url) = (self.client.clone(), self.mirrors.base_url(mirror));
        tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_secs(3), client.head(url).send()).await;
        });
    }

    /// Downloads the archive for `set_id` and returns the path of the validated .osz.
    #[cfg(test)]
    pub async fn fetch(&self, set_id: u32, no_video: bool, preferred: &[MirrorId]) -> Result<PathBuf> {
        self.fetch_named(set_id, no_video, preferred, None).await
    }

    /// Like `fetch`, naming the archive from `hint` when there is one instead of asking a mirror.
    pub async fn fetch_named(
        &self,
        set_id: u32,
        no_video: bool,
        preferred: &[MirrorId],
        hint: Option<&NameHint>,
    ) -> Result<PathBuf> {
        let hinted = hint.and_then(|hint| hint.archive_name(set_id));
        let (token, _guard) = self.register(set_id)?;
        self.emit_state(set_id, DownloadState::Queued, None);
        let permit = tokio::select! {
            permit = self.permits.acquire() => permit.expect("the download semaphore is never closed"),
            _ = token.cancelled() => {
                self.emit_state(set_id, DownloadState::Cancelled, None);
                return Err(AppError::Cancelled);
            }
        };
        self.emit_state(set_id, DownloadState::Downloading, None);
        let result = self.fetch_from_mirrors(set_id, no_video, preferred, hinted.as_deref(), &token).await;
        drop(permit);
        match &result {
            Ok(_) => {}
            Err(AppError::Cancelled) => self.emit_state(set_id, DownloadState::Cancelled, None),
            Err(error) => self.emit_state(set_id, DownloadState::Failed, Some(error.to_string())),
        }
        result
    }

    async fn fetch_from_mirrors(
        &self,
        set_id: u32,
        no_video: bool,
        preferred: &[MirrorId],
        hinted: Option<&str>,
        token: &CancellationToken,
    ) -> Result<PathBuf> {
        let mut failures = Vec::new();
        for mirror in self.mirrors.candidates(preferred, false) {
            match self.try_mirror(mirror, set_id, no_video, hinted, preferred, token).await {
                Ok(path) => {
                    self.mirrors.mark_healthy(mirror);
                    return Ok(path);
                }
                Err((AppError::Cancelled, _)) => return Err(AppError::Cancelled),
                Err((error, transient)) => {
                    if transient {
                        self.mirrors.mark_unhealthy(mirror);
                    }
                    failures.push(format!("{}: {error}", mirror.name()));
                }
            }
        }
        Err(AppError::AllMirrorsFailed(failures.join("; ")))
    }

    /// Some mirrors send no filename, which would leave the archive called `{id}.osz` - and osu!
    /// names the Songs folder after the archive. This looks the set up to name it
    /// `{id} {artist} - {title}.osz`, and runs alongside the download rather than after it: it is
    /// started as soon as the response headers show no name, and its result is picked up once the
    /// file has arrived. It never holds the download up for long (see the timeout).
    fn start_name_lookup(&self, set_id: u32, preferred: &[MirrorId]) -> tokio::task::JoinHandle<Option<String>> {
        let mirrors = self.mirrors.clone();
        let preferred = preferred.to_vec();
        let timeout = self.name_lookup_timeout;
        tokio::spawn(async move {
            let set = tokio::time::timeout(timeout, mirrors.lookup_set(&preferred, set_id)).await.ok()?.ok()?;
            (set.id == set_id).then(|| standard_archive_name(&set)).flatten()
        })
    }

    /// Sends the request and follows redirects itself, so it can read the filename a redirect
    /// announced: osu.direct names the file on its 302, and the storage response that 302 points
    /// at carries no name at all.
    async fn get(
        &self,
        mut url: Url,
        token: &CancellationToken,
    ) -> std::result::Result<(reqwest::Response, Option<String>), (AppError, bool)> {
        const MAX_REDIRECTS: usize = 5;
        let mut announced = None;
        for _ in 0..=MAX_REDIRECTS {
            let response = tokio::select! {
                response = self.client.get(url.clone()).send() => response,
                _ = token.cancelled() => return Err((AppError::Cancelled, false)),
            }
            .map_err(|e| (AppError::BadResponse(format!("request failed: {e}")), true))?;
            if !response.status().is_redirection() {
                return Ok((response, announced));
            }
            announced = announced.or_else(|| disposition_name(response.headers()));
            let next = response
                .headers()
                .get(header::LOCATION)
                .and_then(|location| location.to_str().ok())
                .and_then(|location| url.join(location).ok());
            match next {
                Some(next) => url = next,
                // A redirect with nowhere to go is handed back as the unsuccessful status it is.
                None => return Ok((response, announced)),
            }
        }
        Err((AppError::BadResponse("too many redirects".into()), true))
    }

    /// One attempt against one mirror. The error carries whether the mirror looks unwell
    /// (worth sidelining) as opposed to simply not having this set.
    async fn try_mirror(
        &self,
        mirror: MirrorId,
        set_id: u32,
        no_video: bool,
        hinted: Option<&str>,
        preferred: &[MirrorId],
        token: &CancellationToken,
    ) -> std::result::Result<PathBuf, (AppError, bool)> {
        let url = self.mirrors.download_url(mirror, set_id, no_video);
        let (response, redirect_name) = self.get(url, token).await?;

        let status = response.status();
        if !status.is_success() {
            let transient = !matches!(status.as_u16(), 404 | 410);
            return Err((AppError::BadResponse(format!("HTTP {status}")), transient));
        }
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_ascii_lowercase);
        if let Some(content_type) = content_type {
            // Error pages and rate-limit notices come back as 200 text/html or JSON.
            if content_type.starts_with("text/") || content_type.contains("json") {
                return Err((AppError::InvalidArchive(format!("server sent {content_type}")), true));
            }
        }

        let total = response.content_length();
        // The name the mirror announced (on the file itself, or on a redirect hop), else the
        // caller's hint, else a plain `{id}.osz` plus a lookup that runs while the file downloads.
        let announced = disposition_name(response.headers()).or(redirect_name);
        let lookup = (announced.is_none() && hinted.is_none()).then(|| self.start_name_lookup(set_id, preferred));
        let file_name = announced
            .or_else(|| hinted.map(str::to_string))
            .unwrap_or_else(|| format!("{set_id}.osz"));

        fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| (AppError::from(e), false))?;
        let part = self.dir.join(format!("{set_id}.osz.part"));
        let target = self.dir.join(file_name);

        let outcome = match self.stream_to_file(response, &part, total, set_id, mirror, token).await {
            Ok(()) => validate_osz(&part).await,
            Err(error) => Err(error),
        };
        match outcome {
            Ok(()) => {
                fs::rename(&part, &target)
                    .await
                    .map_err(|e| (AppError::from(e), false))?;
                Ok(match lookup {
                    Some(lookup) => rename_to_looked_up_name(target, lookup).await,
                    None => target,
                })
            }
            Err(error) => {
                if let Some(lookup) = lookup {
                    lookup.abort();
                }
                let _ = fs::remove_file(&part).await;
                let transient = !matches!(error, AppError::Cancelled);
                Err((error, transient))
            }
        }
    }

    async fn stream_to_file(
        &self,
        response: reqwest::Response,
        part: &Path,
        total: Option<u64>,
        set_id: u32,
        mirror: MirrorId,
        token: &CancellationToken,
    ) -> Result<()> {
        let mut file = fs::File::create(part).await?;
        let mut stream = Box::pin(response.bytes_stream());
        let mut received = 0u64;
        let mut last_emit = Instant::now();
        loop {
            let chunk = tokio::select! {
                chunk = stream.next() => chunk,
                _ = token.cancelled() => return Err(AppError::Cancelled),
            };
            match chunk {
                Some(Ok(bytes)) => {
                    file.write_all(&bytes).await?;
                    received += bytes.len() as u64;
                    if last_emit.elapsed() >= PROGRESS_EVERY {
                        last_emit = Instant::now();
                        self.sink.progress(ProgressEvent { set_id, received, total, mirror: mirror.name() });
                    }
                }
                Some(Err(error)) => {
                    return Err(AppError::BadResponse(format!("download interrupted: {error}")));
                }
                None => break,
            }
        }
        file.flush().await?;
        // Windows can't rename a file that is still open.
        drop(file);
        if let Some(total) = total
            && total != received
        {
            return Err(AppError::InvalidArchive(format!("expected {total} bytes, got {received}")));
        }
        self.sink.progress(ProgressEvent { set_id, received, total, mirror: mirror.name() });
        Ok(())
    }
}

/// `filename*=UTF-8''...` (RFC 5987) wins over plain `filename=...`.
pub fn filename_from_disposition(value: &str) -> Option<String> {
    fn after_prefix<'a>(part: &'a str, prefix: &str) -> Option<&'a str> {
        let head = part.get(..prefix.len())?;
        head.eq_ignore_ascii_case(prefix).then(|| &part[prefix.len()..])
    }
    let parts: Vec<&str> = value.split(';').map(str::trim).collect();
    for part in &parts {
        if let Some(rest) = after_prefix(part, "filename*=")
            && let Some((_charset, encoded)) = rest.split_once("''")
        {
            return Some(percent_decode_str(encoded).decode_utf8_lossy().into_owned());
        }
    }
    parts
        .iter()
        .find_map(|part| after_prefix(part, "filename="))
        .map(|name| name.trim_matches('"').to_string())
}

/// A name that is safe on Windows and ends in `.osz`.
pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    let mut name = if trimmed.is_empty() { "beatmapset".to_string() } else { trimmed.to_string() };
    if !name.to_ascii_lowercase().ends_with(".osz") {
        name.push_str(".osz");
    }
    if name.chars().count() > 150 {
        let stem: String = name.chars().take(146).collect();
        name = format!("{}.osz", stem.trim_end());
    }
    name
}

/// `{id} {artist} - {title}.osz`, the name osu! itself gives downloads, so the Songs folder osu!
/// creates from the archive matches the rest of the library. `None` when the set has neither
/// an artist nor a title to name it with.
pub fn standard_archive_name(set: &Beatmapset) -> Option<String> {
    archive_name(set.id, &set.artist, &set.title)
}

fn archive_name(id: u32, artist: &str, title: &str) -> Option<String> {
    let (artist, title) = (artist.trim(), title.trim());
    let label = match (artist.is_empty(), title.is_empty()) {
        (true, true) => return None,
        (false, false) => format!("{artist} - {title}"),
        (false, true) => artist.to_string(),
        (true, false) => title.to_string(),
    };
    Some(sanitize_filename(&format!("{id} {label}")))
}

/// The filename a response announces in its Content-Disposition header, made safe for Windows.
fn disposition_name(headers: &header::HeaderMap) -> Option<String> {
    headers
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(filename_from_disposition)
        .map(|name| sanitize_filename(&name))
}

/// Renames `archive` to the name a lookup found. Any failure keeps the plain name: a slightly
/// untidy Songs folder is better than a failed or delayed download.
async fn rename_to_looked_up_name(archive: PathBuf, lookup: tokio::task::JoinHandle<Option<String>>) -> PathBuf {
    let Some(name) = lookup.await.ok().flatten() else { return archive };
    let target = archive.with_file_name(name);
    if target == archive {
        return archive;
    }
    match fs::rename(&archive, &target).await {
        Ok(()) => target,
        Err(_) => archive,
    }
}

/// A real .osz is a zip: it starts with a local file header and ends with an end-of-archive
/// record. That catches HTML error pages and truncated downloads.
pub fn check_zip_markers(head: &[u8], tail: &[u8], len: u64) -> Result<()> {
    if len < MIN_ARCHIVE_BYTES {
        return Err(AppError::InvalidArchive(format!("only {len} bytes")));
    }
    if !head.starts_with(LOCAL_HEADER) {
        return Err(AppError::InvalidArchive("missing zip header".into()));
    }
    if !tail.windows(END_OF_ARCHIVE.len()).any(|w| w == END_OF_ARCHIVE) {
        return Err(AppError::InvalidArchive("missing end-of-archive marker, download truncated?".into()));
    }
    Ok(())
}

async fn validate_osz(path: &Path) -> Result<()> {
    let mut file = fs::File::open(path).await?;
    let len = file.metadata().await?.len();
    if len < MIN_ARCHIVE_BYTES {
        return Err(AppError::InvalidArchive(format!("only {len} bytes")));
    }
    let mut head = vec![0u8; LOCAL_HEADER.len()];
    file.read_exact(&mut head).await?;
    let tail_len = len.min(TAIL_BYTES);
    file.seek(SeekFrom::End(-(tail_len as i64))).await?;
    let mut tail = vec![0u8; tail_len as usize];
    file.read_exact(&mut tail).await?;
    check_zip_markers(&head, &tail, len)
}

/// Deletes stale `.osz` / `.part` files from the download folder (osu! has long since read
/// them). Only those two extensions are touched.
pub fn cleanup_old(dir: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !(ext.eq_ignore_ascii_case("osz") || ext.eq_ignore_ascii_case("part")) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > max_age);
        if stale {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    fn fake_osz() -> Vec<u8> {
        let mut bytes = b"PK\x03\x04".to_vec();
        bytes.extend([0u8; 40]);
        bytes.extend(b"PK\x05\x06");
        bytes.extend([0u8; 18]);
        bytes
    }

    fn osz_response(file_name: &str) -> ResponseTemplate {
        ResponseTemplate::new(200)
            .insert_header("content-type", "application/x-osu-beatmap-archive")
            .insert_header("content-disposition", format!("attachment; filename=\"{file_name}\""))
            .set_body_bytes(fake_osz())
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<StateEvent>>);

    impl Recorder {
        fn states(&self) -> Vec<DownloadState> {
            self.0.lock().unwrap().iter().map(|e| e.state).collect()
        }
    }

    impl EventSink for Recorder {
        fn state(&self, event: StateEvent) {
            self.0.lock().unwrap().push(event);
        }
        fn progress(&self, _event: ProgressEvent) {}
    }

    struct Rig {
        catboy: MockServer,
        osu: MockServer,
        dir: tempfile::TempDir,
        mirrors: Arc<MirrorSet>,
        recorder: Arc<Recorder>,
        downloader: Arc<Downloader>,
    }

    async fn rig() -> Rig {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let mirrors = Arc::new(MirrorSet::with_bases(&catboy.uri(), &osu.uri()));
        let recorder = Arc::new(Recorder::default());
        let downloader = Arc::new(Downloader::new(mirrors.clone(), dir.path().to_path_buf(), recorder.clone()));
        Rig { catboy, osu, dir, mirrors, recorder, downloader }
    }

    fn files_with_extension(dir: &Path, ext: &str) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == ext))
            .count()
    }

    #[tokio::test]
    async fn downloads_validates_and_names_the_archive() {
        let rig = rig().await;
        Mock::given(method("GET"))
            .and(path("/d/42n"))
            .respond_with(osz_response("42 Artist - Title.osz"))
            .expect(1)
            .mount(&rig.catboy)
            .await;
        let file = rig.downloader.fetch(42, true, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "42 Artist - Title.osz");
        assert_eq!(std::fs::read(&file).unwrap(), fake_osz());
        assert_eq!(files_with_extension(rig.dir.path(), "part"), 0);
        assert_eq!(rig.recorder.states(), [DownloadState::Queued, DownloadState::Downloading]);
    }

    #[tokio::test]
    async fn the_video_variant_is_requested_when_no_video_is_off() {
        let rig = rig().await;
        Mock::given(method("GET"))
            .and(path("/d/42"))
            .respond_with(osz_response("42 A - B.osz"))
            .expect(1)
            .mount(&rig.catboy)
            .await;
        rig.downloader.fetch(42, false, &MirrorId::ALL).await.unwrap();
    }

    #[tokio::test]
    async fn an_html_page_is_rejected_and_the_next_mirror_is_used() {
        let rig = rig().await;
        Mock::given(method("GET"))
            .and(path("/d/9n"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_string("<html>rate limited</html>"),
            )
            .expect(1)
            .mount(&rig.catboy)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/d/9"))
            .and(query_param("noVideo", "true"))
            .respond_with(osz_response("9 A - B.osz"))
            .expect(1)
            .mount(&rig.osu)
            .await;
        let file = rig.downloader.fetch(9, true, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "9 A - B.osz");
        // The mirror that served the page is sidelined; the one that worked now goes first.
        assert_eq!(rig.mirrors.candidates(&MirrorId::ALL, false)[0], MirrorId::OsuDirect);
    }

    #[tokio::test]
    async fn a_mirror_without_the_set_is_skipped_but_not_sidelined() {
        let rig = rig().await;
        // catboy has no mock for this path, so it answers 404.
        Mock::given(method("GET"))
            .and(path("/api/d/5"))
            .respond_with(osz_response("5 A - B.osz"))
            .expect(1)
            .mount(&rig.osu)
            .await;
        rig.downloader.fetch(5, false, &MirrorId::ALL).await.unwrap();
        assert_eq!(rig.mirrors.candidates(&MirrorId::ALL, false)[0], MirrorId::Catboy);
    }

    fn json_response(body: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), "application/json")
    }

    /// A download that carries no Content-Disposition, like osu.direct's.
    async fn mount_unnamed_download(server: &MockServer, set_id: u32) {
        Mock::given(method("GET"))
            .and(path(format!("/d/{set_id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/octet-stream")
                    .set_body_bytes(fake_osz()),
            )
            .mount(server)
            .await;
    }

    async fn mount_set_lookup(server: &MockServer, set_id: u32, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(format!("/api/v2/s/{set_id}")))
            .respond_with(response)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn an_unnamed_archive_is_named_from_the_sets_metadata() {
        let rig = rig().await;
        mount_unnamed_download(&rig.catboy, 11).await;
        mount_set_lookup(
            &rig.catboy,
            11,
            json_response(r#"{"id":11,"title":"Title: Sub?","artist":"The Artist"}"#),
        )
        .await;
        let file = rig.downloader.fetch(11, false, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "11 The Artist - Title_ Sub_.osz");
        assert_eq!(std::fs::read(&file).unwrap(), fake_osz());
        assert!(!rig.dir.path().join("11.osz").exists(), "the plain name is gone");
    }

    #[tokio::test]
    async fn a_name_sent_by_the_mirror_is_kept_without_a_lookup() {
        let rig = rig().await;
        Mock::given(method("GET"))
            .and(path("/d/21"))
            .respond_with(osz_response("21 Mirror - Named.osz"))
            .mount(&rig.catboy)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/s/21"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&rig.catboy)
            .await;
        let file = rig.downloader.fetch(21, false, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "21 Mirror - Named.osz");
    }

    #[tokio::test]
    async fn a_failed_lookup_keeps_the_plain_name_and_the_download_still_succeeds() {
        let rig = rig().await;
        mount_unnamed_download(&rig.catboy, 12).await;
        mount_set_lookup(&rig.catboy, 12, ResponseTemplate::new(500)).await;
        // osu.direct has no mock either, so every mirror's lookup fails.
        let file = rig.downloader.fetch(12, false, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "12.osz");
        assert!(file.is_file());
    }

    #[tokio::test]
    async fn a_lookup_that_answers_for_a_different_set_is_ignored() {
        let rig = rig().await;
        mount_unnamed_download(&rig.catboy, 13).await;
        mount_set_lookup(&rig.catboy, 13, json_response(r#"{"id":99,"title":"Wrong","artist":"Set"}"#)).await;
        let file = rig.downloader.fetch(13, false, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "13.osz");
    }

    #[tokio::test]
    async fn a_lookup_with_no_artist_or_title_keeps_the_plain_name() {
        let rig = rig().await;
        mount_unnamed_download(&rig.catboy, 14).await;
        mount_set_lookup(&rig.catboy, 14, json_response(r#"{"id":14,"title":"","artist":"  "}"#)).await;
        let file = rig.downloader.fetch(14, false, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "14.osz");
    }

    #[tokio::test]
    async fn a_slow_lookup_is_abandoned_instead_of_holding_the_download_up() {
        let catboy = MockServer::start().await;
        let osu = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let mirrors = Arc::new(MirrorSet::with_bases(&catboy.uri(), &osu.uri()));
        let downloader = Downloader::new(mirrors, dir.path().to_path_buf(), Arc::new(Recorder::default()))
            .with_name_lookup_timeout(Duration::from_millis(150));
        mount_unnamed_download(&catboy, 15).await;
        mount_set_lookup(
            &catboy,
            15,
            json_response(r#"{"id":15,"title":"T","artist":"A"}"#).set_delay(Duration::from_secs(3)),
        )
        .await;

        let started = Instant::now();
        let file = downloader.fetch(15, false, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "15.osz");
        assert!(started.elapsed() < Duration::from_secs(2), "took {:?}", started.elapsed());
    }

    #[tokio::test]
    async fn a_hint_names_the_archive_without_any_lookup() {
        let rig = rig().await;
        mount_unnamed_download(&rig.catboy, 51).await;
        Mock::given(method("GET"))
            .and(path("/api/v2/s/51"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&rig.catboy)
            .await;
        let hint = NameHint::from_parts(Some("The Artist".into()), Some("Title".into())).unwrap();
        let file = rig.downloader.fetch_named(51, false, &MirrorId::ALL, Some(&hint)).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "51 The Artist - Title.osz");
        assert_eq!(std::fs::read(&file).unwrap(), fake_osz());
    }

    #[test]
    fn a_hint_needs_an_artist_or_a_title() {
        assert!(NameHint::from_parts(None, None).is_none());
        assert!(NameHint::from_parts(Some("  ".into()), Some(String::new())).is_none());
        let artist_only = NameHint::from_parts(Some("A".into()), None).unwrap();
        assert_eq!(artist_only.archive_name(9).as_deref(), Some("9 A.osz"));
    }

    #[tokio::test]
    async fn the_name_a_redirect_announces_is_used_without_a_lookup() {
        let rig = rig().await;
        // osu.direct's shape: the name rides on the 302, and the storage response it points at has none.
        Mock::given(method("GET"))
            .and(path("/api/d/61"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("content-disposition", "attachment; filename=\"61 Hop - Named.osz\"")
                    .insert_header("location", "/storage/61.osz"),
            )
            .mount(&rig.osu)
            .await;
        Mock::given(method("GET"))
            .and(path("/storage/61.osz"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/octet-stream")
                    .set_body_bytes(fake_osz()),
            )
            .expect(1)
            .mount(&rig.osu)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/s/61"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&rig.osu)
            .await;
        // catboy has no mock, answers 404, and osu.direct serves the set.
        let file = rig.downloader.fetch(61, false, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "61 Hop - Named.osz");
    }

    #[tokio::test]
    async fn a_redirect_loop_gives_up_instead_of_following_forever() {
        let rig = rig().await;
        Mock::given(method("GET"))
            .and(path("/d/62n"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/d/62n"))
            .mount(&rig.catboy)
            .await;
        Mock::given(method("GET")).and(path("/api/d/62")).respond_with(ResponseTemplate::new(500)).mount(&rig.osu).await;
        let error = rig.downloader.fetch(62, true, &MirrorId::ALL).await.unwrap_err();
        assert!(error.to_string().contains("too many redirects"), "{error}");
    }

    /// A server that answers a download with its headers at once and the body `body_delay`
    /// later (a big file in flight), and every other request with a 404.
    fn serve_headers_then_body(body: Vec<u8>, body_delay: Duration) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let body = body.clone();
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut request = [0u8; 2048];
                    let read = stream.read(&mut request).unwrap_or(0);
                    let first_line = String::from_utf8_lossy(&request[..read]).lines().next().unwrap_or("").to_string();
                    if first_line.contains("/d/") {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.flush();
                        std::thread::sleep(body_delay);
                        let _ = stream.write_all(&body);
                    } else {
                        let _ = write!(stream, "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
                    }
                });
            }
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn the_name_lookup_runs_while_the_file_downloads() {
        // The headers come at once but the body takes 300 ms; the lookup also takes 300 ms.
        // One after the other that is 600 ms or more; side by side it is about 300.
        let catboy = serve_headers_then_body(fake_osz(), Duration::from_millis(300));
        let osu = MockServer::start().await;
        mount_set_lookup(
            &osu,
            71,
            json_response(r#"{"id":71,"title":"T","artist":"A"}"#).set_delay(Duration::from_millis(300)),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let mirrors = Arc::new(MirrorSet::with_bases(&catboy, &osu.uri()));
        let downloader = Downloader::new(mirrors, dir.path().to_path_buf(), Arc::new(Recorder::default()));

        let started = Instant::now();
        let file = downloader.fetch(71, false, &MirrorId::ALL).await.unwrap();
        assert_eq!(file.file_name().unwrap(), "71 A - T.osz");
        assert!(started.elapsed() < Duration::from_millis(520), "took {:?}", started.elapsed());
    }

    fn named(id: u32, artist: &str, title: &str) -> Beatmapset {
        Beatmapset { id, artist: artist.into(), title: title.into(), ..Beatmapset::default() }
    }

    #[test]
    fn standard_names_follow_osus_id_artist_title_pattern() {
        let name = |set: Beatmapset| standard_archive_name(&set);
        assert_eq!(name(named(39804, "xi", "FREEDOM DiVE")).as_deref(), Some("39804 xi - FREEDOM DiVE.osz"));
        assert_eq!(name(named(1, "AC/DC", "Who Made Who?")).as_deref(), Some("1 AC_DC - Who Made Who_.osz"));
        assert_eq!(name(named(2, "  Padded ", " Title ")).as_deref(), Some("2 Padded - Title.osz"));
        assert_eq!(name(named(3, "Only Artist", "")).as_deref(), Some("3 Only Artist.osz"));
        assert_eq!(name(named(4, "", "Only Title")).as_deref(), Some("4 Only Title.osz"));
        assert_eq!(name(named(7, "米津玄師", "Lemon")).as_deref(), Some("7 米津玄師 - Lemon.osz"));
        assert_eq!(name(named(5, " ", "")), None);
        let long = name(named(6, "A", &"x".repeat(300))).unwrap();
        assert!(long.starts_with("6 A - x") && long.ends_with(".osz") && long.chars().count() <= 150);
    }

    #[tokio::test]
    async fn a_truncated_archive_fails_cleanly_everywhere() {
        let rig = rig().await;
        let truncated = || {
            let mut bytes = b"PK\x03\x04".to_vec();
            bytes.extend([0u8; 100]);
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(bytes)
        };
        Mock::given(method("GET")).and(path("/d/3n")).respond_with(truncated()).mount(&rig.catboy).await;
        Mock::given(method("GET")).and(path("/api/d/3")).respond_with(truncated()).mount(&rig.osu).await;

        let error = rig.downloader.fetch(3, true, &MirrorId::ALL).await.unwrap_err();
        assert!(matches!(error, AppError::AllMirrorsFailed(_)), "{error}");
        assert!(error.to_string().contains("end-of-archive"), "{error}");
        assert_eq!(files_with_extension(rig.dir.path(), "part"), 0);
        assert_eq!(files_with_extension(rig.dir.path(), "osz"), 0);
        assert!(rig.recorder.states().contains(&DownloadState::Failed));
    }

    #[tokio::test]
    async fn a_too_small_download_is_rejected() {
        let rig = rig().await;
        Mock::given(method("GET"))
            .and(path("/d/8n"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"PK\x03\x04".to_vec()))
            .mount(&rig.catboy)
            .await;
        Mock::given(method("GET")).and(path("/api/d/8")).respond_with(ResponseTemplate::new(500)).mount(&rig.osu).await;
        assert!(rig.downloader.fetch(8, true, &MirrorId::ALL).await.is_err());
        assert_eq!(files_with_extension(rig.dir.path(), "part"), 0);
    }

    #[tokio::test]
    async fn cancelling_stops_a_slow_download_and_duplicates_are_refused() {
        let rig = rig().await;
        Mock::given(method("GET"))
            .and(path("/d/7n"))
            .respond_with(osz_response("7 A - B.osz").set_delay(Duration::from_secs(10)))
            .mount(&rig.catboy)
            .await;
        let downloader = rig.downloader.clone();
        let task = tokio::spawn(async move { downloader.fetch(7, true, &MirrorId::ALL).await });
        tokio::time::sleep(Duration::from_millis(200)).await;

        let second = rig.downloader.fetch(7, true, &MirrorId::ALL).await;
        assert!(matches!(second, Err(AppError::AlreadyRunning(7))));
        assert!(rig.downloader.cancel(7));
        assert!(matches!(task.await.unwrap(), Err(AppError::Cancelled)));
        assert!(rig.recorder.states().contains(&DownloadState::Cancelled));
        assert!(!rig.downloader.cancel(7), "nothing left to cancel");
    }

    #[test]
    fn reads_filenames_from_content_disposition() {
        assert_eq!(
            filename_from_disposition(r#"attachment; filename="39804 xi - FREEDOM DiVE.osz""#).as_deref(),
            Some("39804 xi - FREEDOM DiVE.osz")
        );
        assert_eq!(filename_from_disposition("attachment; filename=plain.osz").as_deref(), Some("plain.osz"));
        assert_eq!(
            filename_from_disposition("attachment; filename=\"fallback.osz\"; filename*=UTF-8''%E3%81%82%20b.osz")
                .as_deref(),
            Some("あ b.osz")
        );
        assert_eq!(filename_from_disposition("attachment"), None);
        assert_eq!(filename_from_disposition("inline; FILENAME=\"X.osz\"").as_deref(), Some("X.osz"));
    }

    #[test]
    fn sanitises_filenames_for_windows() {
        assert_eq!(sanitize_filename("1 A/B: C?.osz"), "1 A_B_ C_.osz");
        assert_eq!(sanitize_filename("..\\..\\evil.osz"), "_.._evil.osz");
        assert_eq!(sanitize_filename("no extension"), "no extension.osz");
        assert_eq!(sanitize_filename("   "), "beatmapset.osz");
        assert_eq!(sanitize_filename("UPPER.OSZ"), "UPPER.OSZ");
        let long = sanitize_filename(&format!("{}.osz", "x".repeat(400)));
        assert!(long.chars().count() <= 150 && long.ends_with(".osz"));
    }

    #[test]
    fn zip_markers_catch_bad_downloads() {
        let osz = fake_osz();
        assert!(check_zip_markers(&osz[..4], &osz, osz.len() as u64).is_ok());
        assert!(check_zip_markers(b"<htm", b"<html>", 6).is_err(), "too short");
        let html = vec![b'<'; 100];
        assert!(check_zip_markers(&html[..4], &html, 100).is_err(), "not a zip");
        let cut = &osz[..30];
        assert!(check_zip_markers(&cut[..4], cut, 30).is_err(), "no end marker");
    }

    #[test]
    fn cleanup_removes_only_old_archives() {
        let dir = tempfile::tempdir().unwrap();
        let old = SystemTime::now() - Duration::from_secs(48 * 3600);
        for (name, age) in [("old.osz", Some(old)), ("old.osz.part", Some(old)), ("fresh.osz", None), ("old.txt", Some(old))] {
            let file = std::fs::File::create(dir.path().join(name)).unwrap();
            if let Some(age) = age {
                file.set_modified(age).unwrap();
            }
        }
        cleanup_old(dir.path(), Duration::from_secs(24 * 3600));
        assert!(!dir.path().join("old.osz").exists());
        assert!(!dir.path().join("old.osz.part").exists());
        assert!(dir.path().join("fresh.osz").exists());
        assert!(dir.path().join("old.txt").exists(), "non-archive files are never touched");
        cleanup_old(&dir.path().join("missing"), Duration::ZERO);
    }
}
