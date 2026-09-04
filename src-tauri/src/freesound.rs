//! Filling the local library from Freesound.
//!
//! The network is touched here and nowhere else in the running application. A game
//! session never makes a request: sounds are downloaded once, into the library, and
//! everything after that works with the internet off.
//!
//! What gets downloaded is the high-quality MP3 preview rather than the original file.
//! That is not a shortcut — the original needs OAuth2, verified against the live API
//! (401 with a plain token), whereas the preview CDN needs no authentication at all. For
//! a door creak played over a table the difference is inaudible, and it keeps the whole
//! feature to one API key with no browser round-trip.

use std::path::PathBuf;

use dndsound_freesound::{Client, Sound as RemoteSound};
use dndsound_sound::probe;
use dndsound_store::sounds::{NewSound, Provenance, Sound};
use dndsound_store::Db;

use crate::error::CommandError;
use crate::library::{sanitize, short_hash};

/// The settings key the API token is stored under.
///
/// Deliberately not a field on `AppSettings`: that struct is serialized wholesale to the
/// frontend on every status poll, and a credential has no business travelling on a
/// channel it does not need to be on.
pub const API_KEY_SETTING: &str = "freesound_api_key";

pub fn stored_key(db: &Db) -> Result<Option<String>, CommandError> {
    let key: Option<String> = db.settings().get(API_KEY_SETTING)?;
    Ok(key.filter(|k| !k.trim().is_empty()))
}

pub fn client(db: &Db) -> Result<Client, CommandError> {
    let key = stored_key(db)?.ok_or_else(|| {
        CommandError::new("freesoundKeyMissing", "Ключ Freesound не налаштовано.")
    })?;
    Ok(Client::new(key))
}

/// Where a downloaded preview lands.
///
/// The Freesound id is part of the filename, so re-importing the same sound overwrites
/// its own file instead of accumulating copies.
pub fn destination(library_dir: &std::path::Path, sound: &RemoteSound) -> PathBuf {
    let stem = sanitize(&sound.name);
    let unique = short_hash(&sound.page_url);
    library_dir
        .join("freesound")
        .join(format!("{}-{}-{}.mp3", stem, sound.id, unique))
}

/// Download one sound and record it in the library.
pub fn import(
    db: &Db,
    client: &Client,
    library_dir: &std::path::Path,
    sound: &RemoteSound,
) -> Result<Sound, CommandError> {
    let target = destination(library_dir, sound);

    client
        .download_preview(sound, &target, |_| {})
        .map_err(|e| CommandError::new("freesoundFailed", e.to_string()))?;

    // Probed rather than trusted: the library's duration and channel counts come from
    // the file on disk everywhere else, and a downloaded file is no more trustworthy
    // than an imported one.
    let metadata = probe(&target).map_err(|e| {
        let _ = std::fs::remove_file(&target);
        CommandError::new(
            "unsupportedSound",
            format!("freesound returned something unplayable: {e}"),
        )
    })?;

    let attribution = if sound.license.requires_attribution() {
        sound.attribution()
    } else {
        String::new()
    };

    let new = NewSound {
        display_name: sound.name.replace(['_', '-'], " "),
        file_path: target.to_string_lossy().to_string(),
        managed: true,
        format: metadata.format,
        duration_ms: metadata.duration_ms,
        sample_rate: metadata.sample_rate,
        channels: metadata.channels,
        provenance: Provenance {
            source: "freesound".to_string(),
            source_id: sound.id.to_string(),
            source_url: sound.page_url.clone(),
            license: sound.license.short_name().to_string(),
            author: sound.username.clone(),
            attribution,
        },
    };

    db.sounds().import(&new).map_err(CommandError::from)
}
