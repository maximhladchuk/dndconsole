//! Installing the bundled sound pack.
//!
//! The application ships a curated manifest of CC0 sounds rather than the audio itself:
//! 82 files, about 3.6 MB, which would bloat the repository and the installer for no
//! reason when they can be fetched once from a CDN that needs no credentials.
//!
//! That is the whole extent of the network in this application. The pack is downloaded
//! on first launch; after that everything — recognition, detection, playback — runs with
//! the internet off, which is the promise the project is built around.
//!
//! Downloading on demand at trigger time was measured and rejected: 230–300 ms per sound
//! on a good connection, on top of the ~800 ms the speech pipeline already costs, and a
//! silent moment at the table whenever the network hiccups.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use dndsound_pack::{Manifest, ManifestSound, THEMES};
use dndsound_sound::probe;
use dndsound_store::sounds::{NewSound, Provenance};
use dndsound_store::Db;
use serde::Serialize;

use crate::error::CommandError;

/// Where a manifest sound is cached.
///
/// Named by Freesound id, so a re-run overwrites its own file rather than accumulating
/// copies, and a half-written file from an interrupted run is replaced rather than
/// imported.
pub fn cache_path(cache_dir: &Path, sound: &ManifestSound) -> PathBuf {
    cache_dir.join(format!("{}.mp3", sound.id))
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    /// Sounds newly downloaded this run.
    pub downloaded: usize,
    /// Sounds already cached and reused.
    pub reused: usize,
    pub groups: Vec<String>,
    /// One failure does not abort the rest: a pack that installs thirteen groups out of
    /// fourteen is worth more than nothing.
    pub failed: Vec<String>,
    /// Sounds from an older pack that are no longer part of it, removed.
    pub pruned: usize,
}

impl InstallReport {
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Is the pack already usable without touching the network?
pub fn is_installed(db: &Db) -> Result<bool, CommandError> {
    let manifest = Manifest::bundled();
    let have = db.sounds().count()? as usize;
    Ok(have >= manifest.sounds.len())
}

/// Take the database lock, recovering a poisoned mutex the way `AppState` does.
fn lock(db: &Mutex<Db>) -> MutexGuard<'_, Db> {
    db.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("database mutex was poisoned by an earlier panic; recovering");
        poisoned.into_inner()
    })
}

/// Download whatever is missing and wire the groups up to their events.
///
/// Takes the mutex rather than a `&Db` so the lock is released between sounds. A full
/// install is a minute of downloading, and holding the database for that whole minute
/// leaves every other screen waiting on its first query — the window looks frozen even
/// though the install is running exactly as it should.
pub fn install(
    db: &Mutex<Db>,
    cache_dir: &Path,
    mut on_progress: impl FnMut(InstallProgress),
) -> Result<InstallReport, CommandError> {
    let manifest = Manifest::bundled();
    let mut report = InstallReport::default();
    let total = manifest.sounds.len();
    let mut done = 0;

    std::fs::create_dir_all(cache_dir)
        .map_err(|e| CommandError::new("io", format!("could not create the sound cache: {e}")))?;

    for theme in THEMES {
        let group = {
            let guard = lock(db);
            match guard.sounds().group_by_name(theme.group_name)? {
                Some(existing) => existing,
                None => guard.sounds().create_group(theme.group_name)?,
            }
        };

        for sound in manifest.sounds_for(theme.event_id) {
            done += 1;
            on_progress(InstallProgress {
                done,
                total,
                current: sound.name.clone(),
            });

            // Fetched with the lock released. Only the import that follows needs it,
            // and that is measured in microseconds against a download's ~250 ms.
            let installed = fetch_one(cache_dir, sound).and_then(|fetched| {
                let guard = lock(db);
                let id = import_one(&guard, sound, &fetched)?;
                guard.sounds().add_to_group(group.id, id)?;
                Ok(fetched.from_cache)
            });

            match installed {
                Ok(true) => report.reused += 1,
                Ok(false) => report.downloaded += 1,
                Err(e) => report.failed.push(format!("{}: {}", sound.name, e.message)),
            }
        }

        // Point the event at the group only once it has something in it, so a failed
        // download never leaves an event aimed at silence.
        let guard = lock(db);
        if !guard.sounds().group_members(group.id)?.is_empty() {
            guard
                .events()
                .set_sound_group(theme.event_id, Some(group.id))?;
            report.groups.push(theme.group_name.to_string());
        }
    }

    report.pruned = prune(db, &manifest)?;

    tracing::info!(
        downloaded = report.downloaded,
        reused = report.reused,
        pruned = report.pruned,
        failed = report.failed.len(),
        "sound pack install finished"
    );
    Ok(report)
}

/// Remove sounds an earlier version of the pack installed and this one does not.
///
/// The library is the application's to manage — there is no import screen and nothing
/// here was chosen by the user — so a sound that has dropped out of the manifest is
/// stale, not personal. This matters beyond tidiness: an earlier pack included CC-BY
/// sounds, and the application now promises CC0 only. Leaving them would make that
/// promise false.
///
/// Only sounds marked as coming from Freesound are considered. Anything imported locally
/// is left strictly alone.
fn prune(db: &Mutex<Db>, manifest: &Manifest) -> Result<usize, CommandError> {
    let wanted: std::collections::HashSet<String> =
        manifest.sounds.iter().map(|s| s.id.to_string()).collect();

    let stale: Vec<_> = lock(db)
        .sounds()
        .list()?
        .into_iter()
        .filter(|sound| {
            sound.provenance.source == "freesound" && !wanted.contains(&sound.provenance.source_id)
        })
        .collect();

    for sound in &stale {
        tracing::info!(
            sound = %sound.display_name,
            license = %sound.provenance.license,
            "removing a sound that is no longer in the pack"
        );
        lock(db).sounds().delete(sound.id)?;
    }

    // Groups an older pack created, now empty and unreferenced.
    let groups = lock(db).sounds().list_groups()?;
    for group in groups {
        let guard = lock(db);
        let empty = guard.sounds().group_members(group.id)?.is_empty();
        let unused = !THEMES.iter().any(|t| t.group_name == group.name);
        if empty && unused {
            guard.sounds().delete_group(group.id)?;
        }
    }

    Ok(stale.len())
}

/// A manifest sound that is on disk and known to be readable, but not yet in the
/// library. Splitting the fetch from the import is what lets the database lock stay
/// released across the slow part.
struct Fetched {
    path: PathBuf,
    metadata: dndsound_sound::SoundMetadata,
    from_cache: bool,
}

/// Get the file, using the cache when it holds something playable. Touches no database.
fn fetch_one(cache_dir: &Path, sound: &ManifestSound) -> Result<Fetched, CommandError> {
    let target = cache_path(cache_dir, sound);
    let mut from_cache = target.is_file();

    if !from_cache {
        download(&sound.preview_url, &target)?;
    }

    // Probed rather than trusted, exactly like a local file: a cached file may have been
    // truncated by a crash, and a truncated MP3 is not something to hand to the mixer.
    let metadata = match probe(&target) {
        Ok(metadata) => metadata,
        Err(e) if from_cache => {
            // Cached and unreadable: replace it once before giving up.
            tracing::warn!(sound = sound.id, %e, "cached file is unreadable, refetching");
            let _ = std::fs::remove_file(&target);
            download(&sound.preview_url, &target)?;
            // It came off the network in the end, and the report should say so.
            from_cache = false;
            probe(&target).map_err(|e| {
                let _ = std::fs::remove_file(&target);
                CommandError::new("unsupportedSound", e.to_string())
            })?
        }
        Err(e) => {
            let _ = std::fs::remove_file(&target);
            return Err(CommandError::new("unsupportedSound", e.to_string()));
        }
    };

    Ok(Fetched {
        path: target,
        metadata,
        from_cache,
    })
}

/// Record a fetched file in the library. Holds the lock for one insert.
fn import_one(db: &Db, sound: &ManifestSound, fetched: &Fetched) -> Result<i64, CommandError> {
    let stored = db.sounds().import(&NewSound {
        display_name: sound.name.replace(['_', '-'], " "),
        file_path: fetched.path.to_string_lossy().to_string(),
        managed: true,
        format: fetched.metadata.format.clone(),
        duration_ms: fetched.metadata.duration_ms,
        sample_rate: fetched.metadata.sample_rate,
        channels: fetched.metadata.channels,
        provenance: Provenance {
            source: "freesound".to_string(),
            source_id: sound.id.to_string(),
            source_url: sound.page_url(),
            license: "CC0".to_string(),
            author: sound.author.clone(),
            // CC0 waives attribution. The author is still recorded, because knowing
            // where a file came from is useful even when nothing obliges it.
            attribution: String::new(),
        },
    })?;

    Ok(stored.id)
}

/// Fetch one preview to a temporary file and rename it into place.
///
/// The rename is the point: an interrupted download leaves a `.part` file, never a
/// truncated MP3 that a later run would happily import.
fn download(url: &str, destination: &Path) -> Result<(), CommandError> {
    use std::io::{Read, Write};

    let response = ureq::get(url)
        .call()
        .map_err(|e| CommandError::new("downloadFailed", format!("{url}: {e}")))?;

    let temporary = destination.with_extension(format!("part-{}", std::process::id()));
    let mut file = std::fs::File::create(&temporary)
        .map_err(|e| CommandError::new("io", format!("{}: {e}", temporary.display())))?;

    let mut reader = response.into_body().into_reader();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| CommandError::new("downloadFailed", e.to_string()))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|e| CommandError::new("io", e.to_string()))?;
    }
    file.sync_all()
        .map_err(|e| CommandError::new("io", e.to_string()))?;
    drop(file);

    std::fs::rename(&temporary, destination)
        .map_err(|e| CommandError::new("io", format!("{}: {e}", destination.display())))?;
    Ok(())
}
