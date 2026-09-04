//! Campaign profiles.
//!
//! A profile scopes which events are enabled and how they are tuned, so a horror
//! one-shot and a high-fantasy campaign can share one sound library without sharing
//! trigger settings. Exactly one profile is active at a time.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{now_ms, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct ProfilesRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ProfilesRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(&self, name: &str, description: &str) -> Result<Profile> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO profiles (name, description, is_active, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?3)",
            params![name, description, now],
        )?;
        self.get(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Profile> {
        self.conn
            .query_row(
                "SELECT id, name, description, is_active, created_at, updated_at
                 FROM profiles WHERE id = ?1",
                [id],
                map_profile,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("profile {id}")))
    }

    pub fn list(&self) -> Result<Vec<Profile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, is_active, created_at, updated_at
             FROM profiles ORDER BY name",
        )?;
        let rows = stmt.query_map([], map_profile)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn active(&self) -> Result<Option<Profile>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, name, description, is_active, created_at, updated_at
                 FROM profiles WHERE is_active = 1 LIMIT 1",
                [],
                map_profile,
            )
            .optional()?)
    }

    /// Make `id` the active profile, deactivating every other one.
    pub fn set_active(&self, id: i64) -> Result<()> {
        // Verify first so activating a missing profile is an error rather than a
        // silent state where nothing is active.
        self.get(id)?;
        self.conn
            .execute("UPDATE profiles SET is_active = 0 WHERE is_active = 1", [])?;
        self.conn.execute(
            "UPDATE profiles SET is_active = 1, updated_at = ?2 WHERE id = ?1",
            params![id, now_ms()],
        )?;
        Ok(())
    }

    pub fn rename(&self, id: i64, name: &str, description: &str) -> Result<Profile> {
        let changed = self.conn.execute(
            "UPDATE profiles SET name = ?2, description = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, name, description, now_ms()],
        )?;
        if changed == 0 {
            return Err(Error::NotFound(format!("profile {id}")));
        }
        self.get(id)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM profiles WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(Error::NotFound(format!("profile {id}")));
        }
        Ok(())
    }

    /// Guarantee the app always has at least one profile, and that one is active.
    /// Called at startup; safe to call repeatedly.
    pub fn ensure_default(&self) -> Result<Profile> {
        if let Some(active) = self.active()? {
            return Ok(active);
        }
        if let Some(first) = self.list()?.into_iter().next() {
            self.set_active(first.id)?;
            return self.get(first.id);
        }
        let profile = self.create("Generic Fantasy", "Default campaign profile")?;
        self.set_active(profile.id)?;
        self.get(profile.id)
    }
}

fn map_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        is_active: row.get::<_, i64>(3)? != 0,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    fn repo(db: &Db) -> ProfilesRepo<'_> {
        ProfilesRepo::new(db.conn())
    }

    #[test]
    fn ensure_default_creates_exactly_one_active_profile() {
        let db = Db::open_in_memory().expect("open");
        let first = repo(&db).ensure_default().expect("ensure");
        let second = repo(&db).ensure_default().expect("ensure again");

        assert_eq!(first.id, second.id, "must not create a second default");
        assert!(second.is_active);
        assert_eq!(repo(&db).list().expect("list").len(), 1);
    }

    #[test]
    fn activating_a_profile_deactivates_the_others() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let a = r.ensure_default().expect("default");
        let b = r
            .create("Curse of Strahd", "gothic horror")
            .expect("create");

        r.set_active(b.id).expect("activate");

        let active = r.active().expect("active").expect("one active");
        assert_eq!(active.id, b.id);
        assert!(!r.get(a.id).expect("get a").is_active);
    }

    #[test]
    fn activating_a_missing_profile_is_an_error_and_changes_nothing() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let a = r.ensure_default().expect("default");

        assert!(r.set_active(9_999).is_err());
        assert_eq!(r.active().expect("active").expect("still active").id, a.id);
    }

    #[test]
    fn names_are_unique() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        r.create("Horror", "").expect("create");
        assert!(r.create("Horror", "").is_err());
    }

    #[test]
    fn delete_and_rename_report_missing_rows() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let p = r.create("Sci-Fi", "").expect("create");

        let renamed = r.rename(p.id, "Cyberpunk", "neon").expect("rename");
        assert_eq!(renamed.name, "Cyberpunk");
        assert_eq!(renamed.description, "neon");

        r.delete(p.id).expect("delete");
        assert!(r.delete(p.id).is_err());
        assert!(r.rename(p.id, "x", "").is_err());
    }
}
