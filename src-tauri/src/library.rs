//! Which beatmapsets are already installed, read straight from osu!'s Songs folder. `osu!.db`
//! is only rewritten when osu! saves it, so it lags behind; folder names (`{id} {artist} -
//! {title}`) are always current. A watcher keeps the set up to date while the app runs.

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::{
    collections::{HashSet, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LibraryDelta {
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
}

impl LibraryDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    /// What changed going from `old` to `new`, sorted for stable output.
    pub fn between(old: &HashSet<u32>, new: &HashSet<u32>) -> Self {
        let mut added: Vec<u32> = new.difference(old).copied().collect();
        let mut removed: Vec<u32> = old.difference(new).copied().collect();
        added.sort_unstable();
        removed.sort_unstable();
        Self { added, removed }
    }
}

/// The set id at the start of a Songs folder name: `"123 Artist - Title"` and `"123"` give 123,
/// while `"123abc"` and `"Artist - Title"` (unsubmitted maps) give nothing.
pub fn set_id_from_folder_name(name: &str) -> Option<u32> {
    let digits = name.bytes().take_while(u8::is_ascii_digit).count();
    let rest = &name[digits..];
    if digits == 0 || !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    name[..digits].parse().ok()
}

pub fn scan_songs(dir: &Path) -> HashSet<u32> {
    let Ok(entries) = std::fs::read_dir(dir) else { return HashSet::new() };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|entry| set_id_from_folder_name(&entry.file_name().to_string_lossy()))
        .collect()
}

/// Windows paths compare case-insensitively, and watcher events may carry a `\\?\` prefix.
fn same_path(a: &Path, b: &Path) -> bool {
    #[cfg(not(windows))]
    { a == b }
    #[cfg(windows)]
    {
    let normal = |p: &Path| {
        p.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    normal(a) == normal(b)
    }
}

pub(crate) fn has_folder_for(dir: &Path, id: u32) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| set_id_from_folder_name(&e.file_name().to_string_lossy()) == Some(id))
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Change {
    Added(u32),
    Removed(u32),
}

/// Brings `ids` in line with what is on disk for one path a watcher reported. Looking at the
/// disk instead of the event kind makes this robust to renames and to events arriving out of order.
fn reconcile(ids: &mut HashSet<u32>, songs_dir: &Path, path: &Path) -> Option<Change> {
    if !path.parent().is_some_and(|parent| same_path(parent, songs_dir)) {
        return None;
    }
    let id = set_id_from_folder_name(&path.file_name()?.to_string_lossy())?;
    if path.is_dir() {
        ids.insert(id).then_some(Change::Added(id))
    } else if ids.contains(&id) && !has_folder_for(songs_dir, id) {
        ids.remove(&id);
        Some(Change::Removed(id))
    } else {
        None
    }
}

struct Watching {
    client: crate::settings::GameClient,
    dir: PathBuf,
    _watcher: RecommendedWatcher,
}

pub struct Library {
    files: Arc<Mutex<HashMap<PathBuf, u32>>>,
    ids: Arc<Mutex<HashSet<u32>>>,
    watching: Mutex<Option<Watching>>,
    on_change: Arc<dyn Fn(LibraryDelta) + Send + Sync>,
}

impl Library {
    pub fn new(on_change: Arc<dyn Fn(LibraryDelta) + Send + Sync>) -> Self {
        Self { files: Arc::default(), ids: Arc::default(), watching: Mutex::new(None), on_change }
    }

    /// Rescans `songs_dir`, makes sure it is being watched, and returns every installed set id.
    /// Anything that changed since the last scan is also reported through `on_change`.
    pub fn sync(&self, songs_dir: Option<&Path>) -> Vec<u32> { self.sync_client(songs_dir, crate::settings::GameClient::Stable) }

    pub fn sync_client(&self, songs_dir: Option<&Path>, client: crate::settings::GameClient) -> Vec<u32> {
        let mut watching = self.watching.lock().unwrap();
        let dir = songs_dir.filter(|dir| dir.is_dir());
        // FSEvents reports canonical paths (/private/var rather than /var on macOS).
        // Watch and compare the same root, including when Songs is reached through a symlink.
        let canonical = dir.map(|dir| if cfg!(unix) || client == crate::settings::GameClient::Lazer {
            std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
        } else { dir.to_path_buf() });
        let dir = canonical.as_deref();
        let changed = watching.as_ref().is_none_or(|watch| watch.client != client || !dir.is_some_and(|dir| same_path(&watch.dir, dir)));
        if changed { *watching = None; }
        if !changed && client == crate::settings::GameClient::Lazer {
            let mut ids = self.ids.lock().unwrap().iter().copied().collect::<Vec<_>>(); ids.sort_unstable(); return ids;
        }
        let file_ids = if client == crate::settings::GameClient::Lazer { dir.map(crate::lazer::scan_files).unwrap_or_default() } else { HashMap::new() };
        let fresh = if client == crate::settings::GameClient::Lazer { file_ids.values().copied().collect() } else { dir.map(scan_songs).unwrap_or_default() };
        *self.files.lock().unwrap() = file_ids;
        let delta = {
            let mut ids = self.ids.lock().unwrap();
            let delta = LibraryDelta::between(&ids, &fresh);
            *ids = fresh.clone();
            delta
        };
        if !delta.is_empty() {
            (self.on_change)(delta);
        }
        match dir {
            Some(dir) if changed => {
                *watching = self.watch(dir, client);
            }
            Some(_) => {}
            None => *watching = None,
        }
        let mut sorted: Vec<u32> = fresh.into_iter().collect();
        sorted.sort_unstable();
        sorted
    }

    fn watch(&self, dir: &Path, client: crate::settings::GameClient) -> Option<Watching> {
        let files = self.files.clone();
        let ids = self.ids.clone();
        let on_change = self.on_change.clone();
        let root = dir.to_path_buf();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(event) = event else { return };
            if client == crate::settings::GameClient::Lazer {
                let delta = {
                    let mut files = files.lock().unwrap();
                    let old: HashSet<_> = files.values().copied().collect();
                    for path in &event.paths {
                        if path.starts_with(&root) && std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
                            // A new hash directory can be populated before inotify registers its
                            // recursive watch. Reconcile that subtree so those files are not lost.
                            files.extend(crate::lazer::scan_directory(&root, path));
                        } else if crate::lazer::stored_file(&root, path) {
                            if let Some(id) = crate::lazer::file_set_id(path) { files.insert(path.clone(), id); }
                            else if !path.exists() { files.remove(path); }
                        } else if path.starts_with(&root) && !path.exists() { files.retain(|file, _| !file.starts_with(path)); }
                    }
                    let fresh = files.values().copied().collect();
                    *ids.lock().unwrap() = fresh;
                    LibraryDelta::between(&old, &ids.lock().unwrap())
                };
                if !delta.is_empty() { on_change(delta); }
                return;
            }
            let mut delta = LibraryDelta::default();
            {
                let mut ids = ids.lock().unwrap();
                for path in &event.paths {
                    match reconcile(&mut ids, &root, path) {
                        Some(Change::Added(id)) => delta.added.push(id),
                        Some(Change::Removed(id)) => delta.removed.push(id),
                        None => {}
                    }
                }
            }
            if !delta.is_empty() {
                on_change(delta);
            }
        })
        .ok()?;
        watcher.watch(dir, if client == crate::settings::GameClient::Lazer { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive }).ok()?;
        Some(Watching { client, dir: dir.to_path_buf(), _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    fn make_dirs(root: &Path, names: &[&str]) {
        for name in names {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
    }

    #[test]
    fn reads_the_set_id_from_folder_names() {
        assert_eq!(set_id_from_folder_name("2464474 Kano - Cherry Pop (Cut Ver)"), Some(2464474));
        assert_eq!(set_id_from_folder_name("2386641"), Some(2386641));
        assert_eq!(set_id_from_folder_name("7 a"), Some(7));
        assert_eq!(set_id_from_folder_name("123abc"), None, "digits must end the id");
        assert_eq!(set_id_from_folder_name("Artist - Title"), None, "unsubmitted maps have no id");
        assert_eq!(set_id_from_folder_name(""), None);
        assert_eq!(set_id_from_folder_name("99999999999 too big"), None);
        assert_eq!(set_id_from_folder_name("-1 negative"), None);
    }

    #[test]
    fn scans_only_folders_with_ids() {
        let dir = tempfile::tempdir().unwrap();
        make_dirs(dir.path(), &["1 A - B", "22", "Unsubmitted - Song", "333x y"]);
        std::fs::write(dir.path().join("44 file.osz"), "").unwrap();
        assert_eq!(scan_songs(dir.path()), HashSet::from([1, 22]));
        assert!(scan_songs(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn deltas_list_what_changed_in_order() {
        let old = HashSet::from([1, 2, 3]);
        let new = HashSet::from([3, 9, 4, 2]);
        assert_eq!(LibraryDelta::between(&old, &new), LibraryDelta { added: vec![4, 9], removed: vec![1] });
        assert!(LibraryDelta::between(&old, &old).is_empty());
    }

    #[test]
    fn reconcile_follows_the_disk_not_the_event() {
        let dir = tempfile::tempdir().unwrap();
        make_dirs(dir.path(), &["5 A - B"]);
        let mut ids = HashSet::new();
        let path = dir.path().join("5 A - B");

        assert_eq!(reconcile(&mut ids, dir.path(), &path), Some(Change::Added(5)));
        assert_eq!(reconcile(&mut ids, dir.path(), &path), None, "already known");

        std::fs::remove_dir(&path).unwrap();
        assert_eq!(reconcile(&mut ids, dir.path(), &path), Some(Change::Removed(5)));
        assert_eq!(reconcile(&mut ids, dir.path(), &path), None, "already gone");
    }

    #[test]
    fn reconcile_ignores_other_places_and_duplicate_folders() {
        let dir = tempfile::tempdir().unwrap();
        make_dirs(dir.path(), &["8 A", "8 A (copy)", "9 Z/inner 5"]);
        let mut ids = HashSet::from([8]);

        // Not a direct child of the Songs folder.
        assert_eq!(reconcile(&mut ids, dir.path(), &dir.path().join("9 Z").join("inner 5")), None);
        // One of two folders for the same set disappears: the set is still installed.
        std::fs::remove_dir(dir.path().join("8 A")).unwrap();
        assert_eq!(reconcile(&mut ids, dir.path(), &dir.path().join("8 A")), None);
        assert!(ids.contains(&8));
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_paths_keep_their_case() {
        assert!(same_path(Path::new("/maps/Songs"), Path::new("/maps/Songs/")));
        assert!(!same_path(Path::new("/maps/Songs"), Path::new("/maps/songs")));
    }

    #[test]
    fn paths_compare_like_windows_does() {
        #[cfg(windows)]
        assert!(same_path(Path::new(r"C:\Games\osu!\Songs"), Path::new(r"c:\games\OSU!\songs\")));
        #[cfg(windows)]
        assert!(same_path(Path::new(r"\\?\C:\Games\Songs"), Path::new(r"C:\Games\Songs")));
        assert!(!same_path(Path::new(r"C:\A"), Path::new(r"C:\B")));
    }

    #[test]
    fn sync_reports_changes_and_the_watcher_sees_new_folders() {
        let dir = tempfile::tempdir().unwrap();
        make_dirs(dir.path(), &["1 One"]);
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let library = Library::new(Arc::new(move |delta| {
            let _ = tx.lock().unwrap().send(delta);
        }));

        // First sync: everything on disk is new.
        assert_eq!(library.sync(Some(dir.path())), vec![1]);
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), LibraryDelta { added: vec![1], removed: vec![] });
        // Nothing changed, so nothing is reported.
        assert_eq!(library.sync(Some(dir.path())), vec![1]);
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());

        // A folder created later arrives through the watcher.
        make_dirs(dir.path(), &["42 Late - Arrival"]);
        let delta = rx.recv_timeout(Duration::from_secs(5)).expect("watcher reported the new folder");
        assert_eq!(delta.added, vec![42]);

        std::fs::remove_dir(dir.path().join("42 Late - Arrival")).unwrap();
        let delta = rx.recv_timeout(Duration::from_secs(5)).expect("watcher reported the removal");
        assert_eq!(delta.removed, vec![42]);
    }

    #[test]
    fn syncing_without_a_songs_folder_empties_the_library() {
        let dir = tempfile::tempdir().unwrap();
        make_dirs(dir.path(), &["1 One"]);
        let library = Library::new(Arc::new(|_| {}));
        assert_eq!(library.sync(Some(dir.path())), vec![1]);
        assert!(library.sync(None).is_empty());
        assert!(library.sync(Some(&dir.path().join("missing"))).is_empty());
    }

    #[test]
    fn lazer_watches_hashed_files_and_keeps_a_set_until_its_last_difficulty_is_removed() {
        let root = tempfile::tempdir().unwrap(); let files = root.path().join("files");
        let first = crate::lazer::fixture_file(&files, &"a".repeat(64), 1);
        let (tx, rx) = mpsc::channel(); let tx = Mutex::new(tx);
        let library = Library::new(Arc::new(move |delta| { let _ = tx.lock().unwrap().send(delta); }));
        assert_eq!(library.sync_client(Some(&files), crate::settings::GameClient::Lazer), vec![1]); rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let duplicate = crate::lazer::fixture_file(&files, &"b".repeat(64), 1);
        let later = crate::lazer::fixture_file(&files, &"c".repeat(64), 42);
        let delta = rx.recv_timeout(Duration::from_secs(10)).expect("lazer watcher reported a new set"); assert_eq!(delta.added, vec![42]);
        std::fs::remove_file(first).unwrap(); assert!(rx.recv_timeout(Duration::from_millis(1500)).is_err());
        std::fs::remove_file(duplicate).unwrap(); let delta = rx.recv_timeout(Duration::from_secs(10)).unwrap(); assert_eq!(delta.removed, vec![1]);
        std::fs::remove_file(later).unwrap(); let delta = rx.recv_timeout(Duration::from_secs(10)).unwrap(); assert_eq!(delta.removed, vec![42]);
        assert!(library.sync_client(Some(&files), crate::settings::GameClient::Lazer).is_empty());
    }
}
