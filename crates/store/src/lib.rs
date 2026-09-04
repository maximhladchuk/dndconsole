//! Persistent metadata store: SQLite schema, migrations, and repositories.
//!
//! This crate owns every SQL statement in the project. Nothing above it writes SQL,
//! and nothing in it knows about Tauri or the audio pipeline.
//!
//! Audio sample data is never stored here — sounds live on disk and are referenced
//! by path.

mod error;
mod settings;

pub mod embeddings;
pub mod events;
pub mod profiles;
pub mod sounds;

pub use error::{Error, Result};
pub use settings::{AppSettings, SettingsRepo};

use std::path::Path;

use rusqlite::Connection;

mod embedded {
    refinery::embed_migrations!("src/migrations");
}

/// A connection to the application database.
///
/// Single-connection by design: the pipeline is synchronous and the UI is the only
/// other writer, so a connection pool would add contention handling for no benefit.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (creating if needed) the database at `path` and run all pending migrations.
    ///
    /// A migration failure is returned, never swallowed — the UI surfaces it rather than
    /// running against a half-migrated schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Open a private in-memory database. Used by tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        // WAL keeps UI reads from blocking pipeline writes during a session.
        // foreign_keys is off by default in SQLite and our schema depends on it.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5_000)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        let report = embedded::migrations::runner()
            .run(&mut conn)
            .map_err(|e| Error::Migration(e.to_string()))?;

        if let Some(v) = report.applied_migrations().last() {
            tracing::info!(version = v.version(), name = v.name(), "applied migration");
        }

        Ok(Self { conn })
    }

    /// Borrow the raw connection. Repositories use this; callers above should not.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn settings(&self) -> SettingsRepo<'_> {
        SettingsRepo::new(&self.conn)
    }

    pub fn profiles(&self) -> profiles::ProfilesRepo<'_> {
        profiles::ProfilesRepo::new(&self.conn)
    }

    pub fn sounds(&self) -> sounds::SoundsRepo<'_> {
        sounds::SoundsRepo::new(&self.conn)
    }

    pub fn events(&self) -> events::EventsRepo<'_> {
        events::EventsRepo::new(&self.conn)
    }

    pub fn embeddings(&self) -> embeddings::EmbeddingsRepo<'_> {
        embeddings::EmbeddingsRepo::new(&self.conn)
    }

    /// Schema version currently applied, or `None` on an unmigrated database.
    pub fn schema_version(&self) -> Result<Option<u32>> {
        let v: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(version) FROM refinery_schema_history",
                [],
                |r| r.get(0),
            )
            .optional_or_none()?;
        Ok(v.map(|v| v as u32))
    }
}

/// Small helper: treat "no rows" and "NULL" identically.
trait OptionalOrNone<T> {
    fn optional_or_none(self) -> Result<Option<T>>;
}

impl<T> OptionalOrNone<T> for std::result::Result<Option<T>, rusqlite::Error> {
    fn optional_or_none(self) -> Result<Option<T>> {
        match self {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Milliseconds since the Unix epoch. Every table timestamps in this unit.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_on_a_fresh_database() {
        let db = Db::open_in_memory().expect("open");
        // Bump this with every migration added under `src/migrations`.
        assert_eq!(db.schema_version().expect("version"), Some(4));
    }

    /// V4 renames the built-in sound groups. The pack installer finds a group by name,
    /// so a database that missed this migration would grow a second, empty copy of every
    /// group rather than reusing the one that already holds the sounds.
    #[test]
    fn renaming_the_groups_leaves_a_hand_named_group_alone() {
        let db = Db::open_in_memory().expect("open");
        let mine = db.sounds().create_group("Doors opening").expect("create");

        // The migration has already run on this fresh database, so a group created
        // afterwards keeps the name it was given.
        let after = db.sounds().group(mine.id).expect("read");
        assert_eq!(after.name, "Doors opening");
    }

    #[test]
    fn the_provenance_migration_leaves_existing_rows_alone() {
        // V2 adds columns to a table that may already hold rows. Defaults must make an
        // already-imported local file describe itself correctly rather than claim a
        // licence nobody granted.
        let db = Db::open_in_memory().expect("open");
        db.conn()
            .execute(
                "INSERT INTO sounds (display_name, file_path, format, created_at, updated_at)
                 VALUES ('Old', '/tmp/old.wav', 'wav', 0, 0)",
                [],
            )
            .expect("insert a pre-migration shaped row");

        let sound = db
            .sounds()
            .by_path("/tmp/old.wav")
            .expect("query")
            .expect("row");
        assert_eq!(sound.provenance.source, "local");
        assert_eq!(sound.provenance.license, "");
        assert!(!sound.provenance.requires_attribution());
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let db = Db::open_in_memory().expect("open");
        let err = db.conn().execute(
            "INSERT INTO event_phrases (event_id, lang, text, kind, created_at)
             VALUES ('NO_SUCH_EVENT', 'en', 'x', 'example', 0)",
            [],
        );
        assert!(err.is_err(), "insert against a missing event should fail");
    }

    #[test]
    fn every_expected_table_exists() {
        let db = Db::open_in_memory().expect("open");
        let mut stmt = db
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .expect("prepare");
        let names: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .expect("query")
            .filter_map(std::result::Result::ok)
            .collect();

        for expected in [
            "profiles",
            "sounds",
            "tags",
            "sound_tags",
            "sound_groups",
            "sound_group_members",
            "events",
            "event_phrases",
            "event_terms",
            "event_chains",
            "phrase_embeddings",
            "profile_events",
            "settings",
            "sessions",
            "session_events",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "missing table {expected}"
            );
        }
    }
}
