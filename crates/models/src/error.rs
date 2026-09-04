use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("unknown model '{0}'")]
    UnknownModel(String),

    #[error("model '{id}' is not downloaded yet")]
    NotDownloaded { id: String },

    #[error("could not download '{id}': {reason}")]
    Download { id: String, reason: String },

    #[error(
        "'{id}' downloaded but failed verification (expected {expected}, got {actual}). \
         The file was deleted; try downloading it again."
    )]
    ChecksumMismatch {
        id: String,
        expected: String,
        actual: String,
    },

    #[error("could not write to the model folder {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
