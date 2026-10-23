use serde::{Serialize, Serializer};

/// Everything a Tauri command can fail with. Serialised as a plain message string so the
/// frontend can show it directly.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("every mirror failed: {0}")]
    AllMirrorsFailed(String),
    #[error("{0}")]
    BadResponse(String),
    #[error("download cancelled")]
    Cancelled,
    #[error("beatmapset {0} is already downloading")]
    AlreadyRunning(u32),
    #[error("the downloaded file is not a valid .osz ({0})")]
    InvalidArchive(String),
    #[error("couldn't find osu! - set its folder in settings")]
    OsuNotFound,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Task(String),
    #[error("hotkey: {0}")]
    Hotkey(String),
    #[error("osu! can't take a map right now: {0}")]
    OsuBusy(crate::handoff::Blocked),
    #[error(
        "osu! hasn't imported it after {seconds} s. If osu! is showing a message, deal with that first, then try again. The download is kept at {path}"
    )]
    ImportStalled { seconds: u64, path: String },
    #[error("Windows is still holding the downloaded file ({0}). Try again in a moment.")]
    ArchiveLocked(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
