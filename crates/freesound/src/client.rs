//! HTTP against the Freesound API.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::query::{SearchPage, SearchQuery, Sound};
use crate::{Error, Result};

const SEARCH_URL: &str = "https://freesound.org/apiv2/search/text/";

/// Only what the library stores is requested. Freesound returns a great deal more.
const FIELDS: &str = "id,name,license,duration,username,previews";

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

pub struct Client {
    api_key: String,
}

impl Client {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }

    pub fn search(&self, query: &SearchQuery) -> Result<SearchPage> {
        let response = ureq::get(SEARCH_URL)
            .header("Authorization", &format!("Token {}", self.api_key))
            .query("query", &query.text)
            .query("filter", query.filter())
            .query("fields", FIELDS)
            .query("page", query.page.to_string())
            .query("page_size", query.page_size.to_string())
            .call()
            .map_err(map_http_error)?;

        let body: serde_json::Value = response
            .into_body()
            .read_json()
            .map_err(|e| Error::Malformed(e.to_string()))?;

        let results = body
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or_else(|| Error::Malformed("no results array".to_string()))?;

        // A result missing a high-quality preview is dropped rather than guessed at:
        // there is nothing to import from it.
        let sounds: Vec<Sound> = results.iter().filter_map(Sound::from_json).collect();

        Ok(SearchPage {
            total: body.get("count").and_then(|c| c.as_u64()).unwrap_or(0),
            page: query.page,
            has_more: body.get("next").is_some_and(|n| !n.is_null()),
            results: sounds,
        })
    }

    /// Fetch one sound by its Freesound id.
    ///
    /// Used by the starter pack, which names the sounds it wants rather than trusting a
    /// search to rank them the same way twice.
    pub fn sound(&self, id: u64) -> Result<Sound> {
        let response = ureq::get(format!("https://freesound.org/apiv2/sounds/{id}/"))
            .header("Authorization", &format!("Token {}", self.api_key))
            .query("fields", FIELDS)
            .call()
            .map_err(map_http_error)?;

        let body: serde_json::Value = response
            .into_body()
            .read_json()
            .map_err(|e| Error::Malformed(e.to_string()))?;

        Sound::from_json(&body)
            .ok_or_else(|| Error::Malformed(format!("sound {id} has no high-quality preview")))
    }

    /// Download a sound's high-quality preview to `destination`.
    ///
    /// The preview CDN takes no authentication, so this deliberately sends none: an API
    /// key does not belong in a request that does not need it.
    pub fn download_preview(
        &self,
        sound: &Sound,
        destination: &Path,
        mut on_progress: impl FnMut(DownloadProgress),
    ) -> Result<PathBuf> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let response = ureq::get(&sound.preview_url)
            .call()
            .map_err(map_http_error)?;

        let total_bytes = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        // Written to a unique temporary first and renamed on success, so an interrupted
        // download never leaves a half a file that looks importable.
        let temporary = destination.with_extension(format!(
            "part-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = fs::File::create(&temporary).map_err(|source| Error::Io {
            path: temporary.clone(),
            source,
        })?;

        let mut reader = response.into_body().into_reader();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut downloaded = 0u64;

        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|e| Error::Request(e.to_string()))?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])
                .map_err(|source| Error::Io {
                    path: temporary.clone(),
                    source,
                })?;
            downloaded += read as u64;
            on_progress(DownloadProgress {
                downloaded_bytes: downloaded,
                total_bytes,
            });
        }

        file.sync_all().map_err(|source| Error::Io {
            path: temporary.clone(),
            source,
        })?;
        drop(file);

        fs::rename(&temporary, destination).map_err(|source| Error::Io {
            path: destination.to_path_buf(),
            source,
        })?;

        tracing::info!(
            sound = sound.id,
            bytes = downloaded,
            path = %destination.display(),
            "imported a freesound preview"
        );

        Ok(destination.to_path_buf())
    }
}

fn map_http_error(error: ureq::Error) -> Error {
    match &error {
        ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => Error::Unauthorized,
        ureq::Error::StatusCode(429) => Error::RateLimited,
        _ => Error::Request(error.to_string()),
    }
}
