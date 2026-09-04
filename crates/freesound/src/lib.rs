//! A read-only Freesound client, used to fill the local sound library.
//!
//! Freesound is a *source*, not a dependency of playing sounds. The application searches
//! and downloads here, once, when the user asks; a game session never touches the
//! network. That is deliberate: the whole point of this project is that running it costs
//! nothing and works with the internet off.
//!
//! Two facts about the API shape everything below, both verified against the live
//! service rather than assumed:
//!
//! * Search and metadata need only a token. Downloading a sound's *original* file
//!   returns 401 without OAuth2.
//! * The high-quality MP3 preview is served from a CDN with no authentication at all.
//!
//! So previews are what gets imported. They are 128 kbps MP3 — indistinguishable from
//! the original for a door creak played over a table, and it keeps the whole feature to
//! a single API key with no browser round-trip.

mod client;
mod license;
mod query;

pub use client::{Client, DownloadProgress};
pub use license::License;
pub use query::{SearchPage, SearchQuery, Sound};

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("freesound rejected the credentials; check the API key in Settings")]
    Unauthorized,

    #[error("freesound rate limit reached; wait a minute and try again")]
    RateLimited,

    #[error("freesound request failed: {0}")]
    Request(String),

    #[error("freesound returned something unexpected: {0}")]
    Malformed(String),

    #[error("could not write {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
