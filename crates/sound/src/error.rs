use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not start the audio output: {0}")]
    Backend(String),

    #[error("could not create mixer track '{track}': {source}")]
    Track {
        track: String,
        #[source]
        source: kira::ResourceLimitReached,
    },

    #[error("sound file not found: {0}")]
    Missing(PathBuf),

    #[error("could not decode '{path}': {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: kira::sound::FromFileError,
    },

    #[error("could not play '{path}': {reason}")]
    Play { path: PathBuf, reason: String },
}
