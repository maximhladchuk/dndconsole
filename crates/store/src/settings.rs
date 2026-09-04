//! Application settings.
//!
//! Settings are stored one key per row rather than as a single JSON blob so that two
//! writers (the UI and the pipeline) can update different settings without clobbering
//! each other. [`AppSettings`] is the typed view: every field has a default, so a
//! database missing a key still produces a complete, valid settings object.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{now_ms, Error, Result};

pub struct SettingsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> SettingsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Read a single setting, or `None` if it has never been written.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;

        match raw {
            None => Ok(None),
            Some(raw) => serde_json::from_str(&raw)
                .map(Some)
                .map_err(|source| Error::Decode {
                    key: key.to_string(),
                    source,
                }),
        }
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let encoded = serde_json::to_string(value).map_err(Error::Encode)?;
        self.conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, encoded, now_ms()],
        )?;
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }

    /// Load the full settings object, filling in defaults for anything unset.
    pub fn load(&self) -> Result<AppSettings> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let mut map = serde_json::Map::new();

        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;

        for row in rows {
            let (key, raw) = row?;
            // An unparseable or unknown key must not break the whole settings load —
            // it is logged and skipped, and the default takes over.
            match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(v) => {
                    map.insert(key, v);
                }
                Err(e) => tracing::warn!(key, error = %e, "ignoring unreadable setting"),
            }
        }

        serde_json::from_value(serde_json::Value::Object(map)).map_err(|source| Error::Decode {
            key: "<app settings>".to_string(),
            source,
        })
    }

    /// Persist every field of `settings` as an individual key.
    pub fn save(&self, settings: &AppSettings) -> Result<()> {
        let value = serde_json::to_value(settings).map_err(Error::Encode)?;
        let obj = value
            .as_object()
            .expect("AppSettings always serializes to a JSON object");

        for (key, value) in obj {
            self.set(key, value)?;
        }
        Ok(())
    }
}

/// Every user-facing setting, with the defaults the app ships with.
///
/// `#[serde(default)]` on the struct is what makes a partially-populated settings table
/// safe: missing keys fall back to [`Default`] instead of failing the load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct AppSettings {
    // --- input -------------------------------------------------------------
    /// Device name as reported by the audio host. `None` means "system default".
    pub input_device: Option<String>,

    // --- models ------------------------------------------------------------
    /// Identifier of the selected local speech model, e.g. `large-v3-turbo-q5_0`.
    pub speech_model: String,
    /// `auto`, `uk`, or `en`.
    pub language: String,

    // --- voice activity detection ------------------------------------------
    pub vad_speech_threshold: f32,
    pub vad_min_speech_ms: u32,
    pub vad_silence_timeout_ms: u32,
    pub vad_pre_roll_ms: u32,
    pub vad_post_roll_ms: u32,
    pub vad_max_segment_ms: u32,

    // --- detection ---------------------------------------------------------
    /// Global bias applied on top of per-event thresholds. Higher is stricter.
    /// The spec's priority order puts a low false-positive rate above recall,
    /// so this starts on the strict side of neutral.
    pub event_sensitivity: f32,

    // --- output ------------------------------------------------------------
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub ambience_volume: f32,
    pub effects_muted: bool,
    /// Ignore the microphone while the app's own sounds are audible.
    ///
    /// Speakers and a microphone in one room form a loop: a thunderclap the app played
    /// is heard, transcribed, and can trigger another. There is no echo canceller here,
    /// so the loop is broken by muting the input for as long as the sound lasts. The
    /// cost is that narration spoken over a sound is lost, which is why it can be turned
    /// off — on headphones there is no loop to break.
    pub suppress_mic_during_playback: bool,

    // --- application -------------------------------------------------------
    pub debug_mode: bool,
    pub start_listening_on_launch: bool,
    /// When true, imported sounds are copied into the app's own library directory
    /// instead of being referenced in place.
    pub managed_library: bool,
    pub active_profile_id: Option<i64>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            input_device: None,

            speech_model: "large-v3-turbo-q5_0".to_string(),
            language: "auto".to_string(),

            // Starting points, to be tuned against real tabletop recordings.
            // See docs/AUDIO_PIPELINE.md.
            vad_speech_threshold: 0.5,
            vad_min_speech_ms: 250,
            vad_silence_timeout_ms: 700,
            vad_pre_roll_ms: 300,
            vad_post_roll_ms: 200,
            vad_max_segment_ms: 5_000,

            event_sensitivity: 0.5,

            master_volume: 1.0,
            sfx_volume: 1.0,
            ambience_volume: 0.7,
            effects_muted: false,
            // On by default: the common setup is a laptop with built-in speakers and a
            // microphone a foot apart, where the loop is loud enough to trigger itself.
            suppress_mic_during_playback: true,

            debug_mode: false,
            start_listening_on_launch: false,
            managed_library: false,
            active_profile_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    #[test]
    fn defaults_load_from_an_empty_database() {
        let db = Db::open_in_memory().expect("open");
        assert_eq!(db.settings().load().expect("load"), AppSettings::default());
    }

    #[test]
    fn settings_round_trip() {
        let db = Db::open_in_memory().expect("open");
        let settings = AppSettings {
            input_device: Some("HyperX QuadCast".to_string()),
            master_volume: 0.42,
            debug_mode: true,
            ..AppSettings::default()
        };

        db.settings().save(&settings).expect("save");
        assert_eq!(db.settings().load().expect("load"), settings);
    }

    #[test]
    fn a_partially_populated_table_still_loads() {
        let db = Db::open_in_memory().expect("open");
        db.settings().set("master_volume", &0.25_f32).expect("set");

        let loaded = db.settings().load().expect("load");
        assert_eq!(loaded.master_volume, 0.25);
        // Everything else fell back to its default.
        assert_eq!(loaded.language, AppSettings::default().language);
    }

    #[test]
    fn an_unreadable_row_is_skipped_rather_than_failing_the_load() {
        let db = Db::open_in_memory().expect("open");
        db.conn()
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('master_volume', 'not json', 0)",
                [],
            )
            .expect("insert");

        let loaded = db.settings().load().expect("load should not fail");
        assert_eq!(loaded.master_volume, AppSettings::default().master_volume);
    }

    #[test]
    fn individual_keys_round_trip() {
        let db = Db::open_in_memory().expect("open");
        let repo = db.settings();

        assert_eq!(repo.get::<String>("missing").expect("get"), None);
        repo.set("language", &"uk".to_string()).expect("set");
        assert_eq!(
            repo.get::<String>("language").expect("get").as_deref(),
            Some("uk")
        );

        repo.delete("language").expect("delete");
        assert_eq!(repo.get::<String>("language").expect("get"), None);
    }
}
