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
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
