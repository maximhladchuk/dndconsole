//! Downloading and verifying model files.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::catalog::{self, ModelSpec};
use crate::{Error, Result};

static NEXT_TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Progress of an in-flight download, reported to the UI.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

/// A directory of downloaded models.
pub struct ModelStore {
    dir: PathBuf,
}

impl ModelStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where a model lives once downloaded, whether or not it is there yet.
    pub fn path_of(&self, spec: &ModelSpec) -> PathBuf {
        self.dir.join(spec.file_name)
    }

    pub fn is_downloaded(&self, spec: &ModelSpec) -> bool {
        self.path_of(spec).is_file()
    }

    /// Bytes a downloaded model occupies, or `None` if it is not present.
    pub fn size_on_disk(&self, spec: &ModelSpec) -> Option<u64> {
        fs::metadata(self.path_of(spec)).ok().map(|m| m.len())
    }

    /// The path to a model that must already be present.
    pub fn require(&self, id: &str) -> Result<PathBuf> {
        let spec = catalog::find(id).ok_or_else(|| Error::UnknownModel(id.to_string()))?;
        let path = self.path_of(spec);
        if path.is_file() {
            Ok(path)
        } else {
            Err(Error::NotDownloaded { id: id.to_string() })
        }
    }

    /// Download a model if it is not already here.
    ///
    /// Writes to a temporary file and renames on success, so an interrupted download
    /// can never leave a half-written model that looks valid. `on_progress` is called
    /// as bytes arrive.
    pub fn ensure(
        &self,
        id: &str,
        mut on_progress: impl FnMut(DownloadProgress),
    ) -> Result<PathBuf> {
        let spec = catalog::find(id).ok_or_else(|| Error::UnknownModel(id.to_string()))?;
        let target = self.path_of(spec);

        if target.is_file() {
            return Ok(target);
        }

        fs::create_dir_all(&self.dir).map_err(|source| Error::Io {
            path: self.dir.clone(),
            source,
        })?;

        tracing::info!(id = spec.id, url = spec.url, "downloading model");

        let response = ureq::get(spec.url).call().map_err(|e| Error::Download {
            id: id.to_string(),
            reason: e.to_string(),
        })?;

        let total_bytes = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        // Unique per call: two threads (or two processes) downloading the same model
        // must not write into the same partial file. Tests hit this immediately.
        let temporary = target.with_extension(format!(
            "part-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut file = fs::File::create(&temporary).map_err(|source| Error::Io {
            path: temporary.clone(),
            source,
        })?;

        let mut reader = response.into_body().into_reader();
        let mut buffer = vec![0u8; 256 * 1024];
        let mut downloaded = 0u64;
        let mut hasher = Sha256::new();

        loop {
            let read = reader.read(&mut buffer).map_err(|e| Error::Download {
                id: id.to_string(),
                reason: e.to_string(),
            })?;
            if read == 0 {
                break;
            }

            hasher.update(&buffer[..read]);
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

        file.flush().map_err(|source| Error::Io {
            path: temporary.clone(),
            source,
        })?;
        drop(file);

        // A truncated download is worse than a failed one: it looks installed and then
        // fails deep inside an inference call.
        if let Some(expected) = total_bytes {
            if downloaded != expected {
                let _ = fs::remove_file(&temporary);
                return Err(Error::Download {
                    id: id.to_string(),
                    reason: format!("expected {expected} bytes but received {downloaded}"),
                });
            }
        }

        // Rename is atomic, so a concurrent download of the same model ends with one
        // valid file rather than a torn one.
        fs::rename(&temporary, &target).map_err(|source| Error::Io {
            path: target.clone(),
            source,
        })?;

        let digest = to_hex(&hasher.finalize());
        tracing::info!(id = spec.id, bytes = downloaded, sha256 = %digest, "model downloaded");
        self.write_digest(spec, &digest)?;

        Ok(target)
    }

    /// Recompute a downloaded model's digest and compare it with the one recorded when
    /// it was fetched. This is what catches a file corrupted on disk later.
    pub fn verify(&self, id: &str) -> Result<String> {
        let spec = catalog::find(id).ok_or_else(|| Error::UnknownModel(id.to_string()))?;
        let path = self.require(id)?;

        let actual = sha256_of(&path)?;

        if let Some(expected) = self.read_digest(spec) {
            if expected != actual {
                return Err(Error::ChecksumMismatch {
                    id: id.to_string(),
                    expected,
                    actual,
                });
            }
        } else {
            // First verification of a file that predates digest recording.
            self.write_digest(spec, &actual)?;
        }

        Ok(actual)
    }

    /// Delete a downloaded model and its digest.
    pub fn remove(&self, id: &str) -> Result<()> {
        let spec = catalog::find(id).ok_or_else(|| Error::UnknownModel(id.to_string()))?;
        let path = self.path_of(spec);

        if path.exists() {
            fs::remove_file(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
        }
        let _ = fs::remove_file(self.digest_path(spec));
        Ok(())
    }

    fn digest_path(&self, spec: &ModelSpec) -> PathBuf {
        self.dir.join(format!("{}.sha256", spec.file_name))
    }

    fn write_digest(&self, spec: &ModelSpec, digest: &str) -> Result<()> {
        let path = self.digest_path(spec);
        fs::write(&path, digest).map_err(|source| Error::Io { path, source })
    }

    fn read_digest(&self, spec: &ModelSpec) -> Option<String> {
        fs::read_to_string(self.digest_path(spec))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| s.len() == 64)
    }
}

fn sha256_of(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];

    loop {
        let read = file.read(&mut buffer).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(to_hex(&hasher.finalize()))
}

/// sha2 0.11 returns a byte array rather than something that formats as hex.
fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> ModelStore {
        let dir =
            std::env::temp_dir().join(format!("dndsound-models-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        ModelStore::new(dir)
    }

    #[test]
    fn an_unknown_model_is_reported_by_name() {
        let store = store("unknown");
        assert!(matches!(
            store.ensure("no-such-model", |_| {}),
            Err(Error::UnknownModel(_))
        ));
        assert!(matches!(
            store.require("no-such-model"),
            Err(Error::UnknownModel(_))
        ));
    }

    #[test]
    fn a_model_that_is_not_downloaded_says_so_rather_than_returning_a_bad_path() {
        let store = store("missing");
        assert!(!store.is_downloaded(catalog::find("silero-vad-16k").expect("spec")));
        assert!(matches!(
            store.require("silero-vad-16k"),
            Err(Error::NotDownloaded { .. })
        ));
        assert_eq!(
            store.size_on_disk(catalog::find("silero-vad-16k").expect("spec")),
            None
        );
    }

    #[test]
    fn digests_round_trip_and_detect_corruption() {
        let store = store("digest");
        let spec = catalog::find("silero-vad-16k").expect("spec");
        fs::create_dir_all(store.dir()).expect("dir");

        fs::write(store.path_of(spec), b"pretend model bytes").expect("write");

        // First verify records the digest.
        let first = store.verify("silero-vad-16k").expect("verify");
        assert_eq!(first.len(), 64);

        // Same bytes verify again.
        assert_eq!(store.verify("silero-vad-16k").expect("verify"), first);

        // Changed bytes are caught.
        fs::write(store.path_of(spec), b"corrupted").expect("write");
        assert!(matches!(
            store.verify("silero-vad-16k"),
            Err(Error::ChecksumMismatch { .. })
        ));

        store.remove("silero-vad-16k").expect("remove");
        assert!(!store.is_downloaded(spec));

        let _ = fs::remove_dir_all(store.dir());
    }

    #[test]
    fn removing_a_model_that_was_never_downloaded_is_harmless() {
        let store = store("remove-missing");
        assert!(store.remove("silero-vad-16k").is_ok());
    }
}
