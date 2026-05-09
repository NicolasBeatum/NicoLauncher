use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Hash mismatch for {file}: expected {expected}, got {actual}")]
    HashMismatch {
        file: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("Java not found: {0}")]
    JavaNotFound(String),

    #[error("Minecraft version not found: {0}")]
    VersionNotFound(String),

    #[error("Manifest error: {0}")]
    Manifest(String),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("{0}")]
    Other(String),
}

// Allow reqwest::Error to be referenced without direct dep in this crate
// (it only appears in the Http variant from dependent crates that re-export)
impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Other(s)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
