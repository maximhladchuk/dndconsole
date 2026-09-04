//! Sound library metadata: files, groups, membership and tags.
//!
//! The database records *where* a sound is and *what it is like*. It never stores the
//! audio itself, and it never copies files — that belongs to the layer above, which
//! knows about the managed-vs-referenced library setting.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// The columns `map_sound` reads, in the order it reads them.
///
/// A macro rather than a constant so `concat!` can build the queries from it: the list
/// had been written out by hand in three places, and adding provenance to two of them
/// left the third returning thirteen columns to a mapper expecting nineteen. SQLite
/// answers that with `InvalidColumnIndex` at runtime, in whichever query was forgotten.
macro_rules! sound_fields {
    () => {
        "s.id, s.display_name, s.file_path, s.managed, s.format, s.duration_ms,
         s.sample_rate, s.channels, s.volume, s.weight, s.enabled, s.favorite, s.missing,
         s.source, s.source_id, s.source_url, s.license, s.author, s.attribution"
    };
}

use crate::{now_ms, Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sound {
    pub id: i64,
    pub display_name: String,
    pub file_path: String,
    /// True when the app copied this file into its own library directory.
    pub managed: bool,
    pub format: String,
    pub duration_ms: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    pub volume: f32,
    pub weight: f32,
    pub enabled: bool,
    pub favorite: bool,
    /// Set when the file could not be found on disk at import or scan time.
    pub missing: bool,
    pub provenance: Provenance,
}

/// Where a sound came from, and what its licence obliges.
///
/// `Default` is a locally imported file: the application knows nothing about its terms
/// and says so, rather than claiming a licence the user never granted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// `local` or `freesound`.
    pub source: String,
    /// Identifier within that source. Empty for local files.
    pub source_id: String,
    pub source_url: String,
    /// Short licence name, e.g. `CC0` or `CC-BY`. Empty when unknown.
    pub license: String,
    pub author: String,
    /// The ready-made credit line, when the licence requires one.
    pub attribution: String,
}

impl Provenance {
    pub fn local() -> Self {
        Self {
            source: "local".to_string(),
            ..Self::default()
        }
    }

    pub fn requires_attribution(&self) -> bool {
        !self.attribution.is_empty()
    }
}

/// Everything needed to record a newly imported file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSound {
    pub display_name: String,
    pub file_path: String,
    pub managed: bool,
    pub format: String,
    pub duration_ms: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundGroup {
    pub id: i64,
    pub name: String,
    /// 'random' | 'weighted' | 'sequential'
    pub selection_mode: String,
    pub prevent_repeat: bool,
    pub volume: f32,
}

pub struct SoundsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> SoundsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ------------------------------------------------------------------ sounds --

    /// Record an imported file.
    ///
    /// `file_path` is unique, so importing the same file twice updates the existing row
    /// instead of creating a duplicate, rather than duplicating
    /// audio unnecessarily.
    pub fn import(&self, new: &NewSound) -> Result<Sound> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO sounds
                 (display_name, file_path, managed, format, duration_ms, sample_rate,
                  channels, created_at, updated_at,
                  source, source_id, source_url, license, author, attribution)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(file_path) DO UPDATE SET
                 display_name = excluded.display_name,
                 managed      = excluded.managed,
                 format       = excluded.format,
                 duration_ms  = excluded.duration_ms,
                 sample_rate  = excluded.sample_rate,
                 channels     = excluded.channels,
                 missing      = 0,
                 updated_at   = excluded.updated_at,
                 source       = excluded.source,
                 source_id    = excluded.source_id,
                 source_url   = excluded.source_url,
                 license      = excluded.license,
                 author       = excluded.author,
                 attribution  = excluded.attribution",
            params![
                new.display_name,
                new.file_path,
                new.managed as i64,
                new.format,
                new.duration_ms,
                new.sample_rate,
                new.channels,
                now,
                new.provenance.source,
                new.provenance.source_id,
                new.provenance.source_url,
                new.provenance.license,
                new.provenance.author,
                new.provenance.attribution,
            ],
        )?;

        self.by_path(&new.file_path)?
            .ok_or_else(|| Error::NotFound(format!("sound at {}", new.file_path)))
    }

    pub fn get(&self, id: i64) -> Result<Sound> {
        self.conn
            .query_row(&format!("{SOUND_COLUMNS} WHERE id = ?1"), [id], map_sound)
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("sound {id}")))
    }

    pub fn by_path(&self, path: &str) -> Result<Option<Sound>> {
        Ok(self
            .conn
            .query_row(
                &format!("{SOUND_COLUMNS} WHERE file_path = ?1"),
                [path],
                map_sound,
            )
            .optional()?)
    }

    pub fn list(&self) -> Result<Vec<Sound>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SOUND_COLUMNS} ORDER BY display_name COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map([], map_sound)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM sounds", [], |r| r.get(0))?)
    }

    pub fn rename(&self, id: i64, display_name: &str) -> Result<Sound> {
        self.update_columns(id, "display_name = ?2", params![id, display_name])?;
        self.get(id)
    }

    pub fn set_volume(&self, id: i64, volume: f32) -> Result<Sound> {
        self.update_columns(id, "volume = ?2", params![id, volume])?;
        self.get(id)
    }

    pub fn set_weight(&self, id: i64, weight: f32) -> Result<Sound> {
        self.update_columns(id, "weight = ?2", params![id, weight])?;
        self.get(id)
    }

    pub fn set_enabled(&self, id: i64, enabled: bool) -> Result<Sound> {
        self.update_columns(id, "enabled = ?2", params![id, enabled as i64])?;
        self.get(id)
    }

    pub fn set_favorite(&self, id: i64, favorite: bool) -> Result<Sound> {
        self.update_columns(id, "favorite = ?2", params![id, favorite as i64])?;
        self.get(id)
    }

    /// Flag a file that has disappeared from disk, so the UI can show it as broken
    /// instead of failing silently at play time.
    pub fn set_missing(&self, id: i64, missing: bool) -> Result<Sound> {
        self.update_columns(id, "missing = ?2", params![id, missing as i64])?;
        self.get(id)
    }

    /// Remove the metadata row. Never touches the file on disk.
    pub fn delete(&self, id: i64) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM sounds WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(Error::NotFound(format!("sound {id}")));
        }
        Ok(())
    }

    fn update_columns(
        &self,
        id: i64,
        assignment: &str,
        values: impl rusqlite::Params,
    ) -> Result<()> {
        let sql = format!("UPDATE sounds SET {assignment}, updated_at = strftime('%s','now') * 1000 WHERE id = ?1");
        let changed = self.conn.execute(&sql, values)?;
        if changed == 0 {
            return Err(Error::NotFound(format!("sound {id}")));
        }
        Ok(())
    }

    // -------------------------------------------------------------------- tags --

    pub fn set_tags(&self, sound_id: i64, tags: &[String]) -> Result<()> {
        self.get(sound_id)?;
        self.conn
            .execute("DELETE FROM sound_tags WHERE sound_id = ?1", [sound_id])?;

        for tag in tags {
            let tag = tag.trim().to_lowercase();
            if tag.is_empty() {
                continue;
            }
            self.conn.execute(
                "INSERT INTO tags (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
                [&tag],
            )?;
            let tag_id: i64 =
                self.conn
                    .query_row("SELECT id FROM tags WHERE name = ?1", [&tag], |r| r.get(0))?;
            self.conn.execute(
                "INSERT INTO sound_tags (sound_id, tag_id) VALUES (?1, ?2)
                 ON CONFLICT DO NOTHING",
                params![sound_id, tag_id],
            )?;
        }
        Ok(())
    }

    pub fn tags(&self, sound_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name FROM tags t
             JOIN sound_tags st ON st.tag_id = t.id
             WHERE st.sound_id = ?1
             ORDER BY t.name",
        )?;
        let rows = stmt.query_map([sound_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Sounds carrying every one of `tags`. The basis for contextual selection later.
    pub fn by_tags(&self, tags: &[String]) -> Result<Vec<Sound>> {
        if tags.is_empty() {
            return self.list();
        }
        let normalized: Vec<String> = tags.iter().map(|t| t.trim().to_lowercase()).collect();
        let placeholders = vec!["?"; normalized.len()].join(", ");
        let sql = format!(
            concat!(
                "SELECT ",
                sound_fields!(),
                "
             FROM sounds s
             JOIN sound_tags st ON st.sound_id = s.id
             JOIN tags t ON t.id = st.tag_id
             WHERE t.name IN ({0})
             GROUP BY s.id
             HAVING COUNT(DISTINCT t.name) = {1}
             ORDER BY s.display_name COLLATE NOCASE"
            ),
            // Named inline captures are not available here: the format string is built
            // by `concat!`, so the arguments have to be positional.
            placeholders,
            normalized.len()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(normalized.iter()), map_sound)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    // ------------------------------------------------------------------ groups --

    pub fn create_group(&self, name: &str) -> Result<SoundGroup> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO sound_groups (name, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![name, now],
        )?;
        self.group(self.conn.last_insert_rowid())
    }

    pub fn group(&self, id: i64) -> Result<SoundGroup> {
        self.conn
            .query_row(&format!("{GROUP_COLUMNS} WHERE id = ?1"), [id], map_group)
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("sound group {id}")))
    }

    pub fn group_by_name(&self, name: &str) -> Result<Option<SoundGroup>> {
        Ok(self
            .conn
            .query_row(
                &format!("{GROUP_COLUMNS} WHERE name = ?1"),
                [name],
                map_group,
            )
            .optional()?)
    }

    pub fn list_groups(&self) -> Result<Vec<SoundGroup>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{GROUP_COLUMNS} ORDER BY name COLLATE NOCASE"))?;
        let rows = stmt.query_map([], map_group)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn update_group(
        &self,
        id: i64,
        name: &str,
        selection_mode: &str,
        prevent_repeat: bool,
        volume: f32,
    ) -> Result<SoundGroup> {
        if !matches!(selection_mode, "random" | "weighted" | "sequential") {
            return Err(Error::NotFound(format!(
                "unknown selection mode '{selection_mode}'"
            )));
        }
        let changed = self.conn.execute(
            "UPDATE sound_groups
             SET name = ?2, selection_mode = ?3, prevent_repeat = ?4, volume = ?5, updated_at = ?6
             WHERE id = ?1",
            params![
                id,
                name,
                selection_mode,
                prevent_repeat as i64,
                volume,
                now_ms()
            ],
        )?;
        if changed == 0 {
            return Err(Error::NotFound(format!("sound group {id}")));
        }
        self.group(id)
    }

    pub fn delete_group(&self, id: i64) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM sound_groups WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(Error::NotFound(format!("sound group {id}")));
        }
        Ok(())
    }

    pub fn add_to_group(&self, group_id: i64, sound_id: i64) -> Result<()> {
        self.group(group_id)?;
        self.get(sound_id)?;

        let next_position: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM sound_group_members WHERE group_id = ?1",
            [group_id],
            |r| r.get(0),
        )?;

        self.conn.execute(
            "INSERT INTO sound_group_members (group_id, sound_id, position)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(group_id, sound_id) DO NOTHING",
            params![group_id, sound_id, next_position],
        )?;
        Ok(())
    }

    pub fn remove_from_group(&self, group_id: i64, sound_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sound_group_members WHERE group_id = ?1 AND sound_id = ?2",
            params![group_id, sound_id],
        )?;
        Ok(())
    }

    /// Members in playback order. Disabled sounds are included; filtering them is the
    /// caller's decision, and the editor needs to see them.
    pub fn group_members(&self, group_id: i64) -> Result<Vec<Sound>> {
        let mut stmt = self.conn.prepare(concat!(
            "SELECT ",
            sound_fields!(),
            "
             FROM sounds s
             JOIN sound_group_members m ON m.sound_id = s.id
             WHERE m.group_id = ?1
             ORDER BY m.position, s.id",
        ))?;
        let rows = stmt.query_map([group_id], map_sound)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

const SOUND_COLUMNS: &str = concat!("SELECT ", sound_fields!(), " FROM sounds s");

const GROUP_COLUMNS: &str =
    "SELECT id, name, selection_mode, prevent_repeat, volume FROM sound_groups";

fn map_sound(row: &rusqlite::Row<'_>) -> rusqlite::Result<Sound> {
    Ok(Sound {
        id: row.get(0)?,
        display_name: row.get(1)?,
        file_path: row.get(2)?,
        managed: row.get::<_, i64>(3)? != 0,
        format: row.get(4)?,
        duration_ms: row.get(5)?,
        sample_rate: row.get(6)?,
        channels: row.get(7)?,
        volume: row.get(8)?,
        weight: row.get(9)?,
        enabled: row.get::<_, i64>(10)? != 0,
        favorite: row.get::<_, i64>(11)? != 0,
        missing: row.get::<_, i64>(12)? != 0,
        provenance: Provenance {
            source: row.get(13)?,
            source_id: row.get(14)?,
            source_url: row.get(15)?,
            license: row.get(16)?,
            author: row.get(17)?,
            attribution: row.get(18)?,
        },
    })
}

fn map_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<SoundGroup> {
    Ok(SoundGroup {
        id: row.get(0)?,
        name: row.get(1)?,
        selection_mode: row.get(2)?,
        prevent_repeat: row.get::<_, i64>(3)? != 0,
        volume: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    fn new_sound(name: &str, path: &str) -> NewSound {
        NewSound {
            display_name: name.to_string(),
            file_path: path.to_string(),
            managed: false,
            format: "wav".to_string(),
            duration_ms: Some(1400),
            sample_rate: Some(44_100),
            channels: Some(1),
            provenance: Provenance::local(),
        }
    }

    /// Every query that feeds `map_sound` must return the same columns.
    ///
    /// The three of them are built from one macro precisely so this cannot drift, and
    /// this exercises all three paths — a mismatch surfaces as `InvalidColumnIndex` in
    /// whichever query was missed, not at compile time.
    #[test]
    fn every_query_that_maps_a_sound_returns_the_same_columns() {
        let db = Db::open_in_memory().expect("db");
        let r = repo(&db);

        let sound = r
            .import(&new_sound("Door", "/tmp/door.wav"))
            .expect("import");
        r.set_tags(sound.id, &["wood".to_string()]).expect("tags");
        let group = r.create_group("Doors").expect("group");
        r.add_to_group(group.id, sound.id).expect("add");

        // Each of these goes through a different SELECT.
        assert_eq!(r.list().expect("list").len(), 1);
        assert_eq!(r.by_tags(&["wood".to_string()]).expect("by tags").len(), 1);
        assert_eq!(r.group_members(group.id).expect("members").len(), 1);
        assert_eq!(r.get(sound.id).expect("get").id, sound.id);
    }

    fn repo(db: &Db) -> SoundsRepo<'_> {
        SoundsRepo::new(db.conn())
    }

    #[test]
    fn importing_the_same_path_twice_updates_rather_than_duplicates() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);

        let first = r
            .import(&new_sound("Door 1", "/sounds/door_01.wav"))
            .expect("import");
        let second = r
            .import(&new_sound("Door One", "/sounds/door_01.wav"))
            .expect("reimport");

        assert_eq!(first.id, second.id);
        assert_eq!(second.display_name, "Door One");
        assert_eq!(r.count().expect("count"), 1);
    }

    #[test]
    fn reimporting_clears_the_missing_flag() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);

        let sound = r
            .import(&new_sound("Door", "/sounds/door.wav"))
            .expect("import");
        r.set_missing(sound.id, true).expect("mark missing");
        assert!(r.get(sound.id).expect("get").missing);

        r.import(&new_sound("Door", "/sounds/door.wav"))
            .expect("reimport");
        assert!(!r.get(sound.id).expect("get").missing, "found again");
    }

    #[test]
    fn sound_fields_round_trip() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let s = r
            .import(&new_sound("Door", "/sounds/door.wav"))
            .expect("import");

        assert_eq!(
            r.rename(s.id, "Old Door").expect("rename").display_name,
            "Old Door"
        );
        assert_eq!(r.set_volume(s.id, 0.5).expect("volume").volume, 0.5);
        assert_eq!(r.set_weight(s.id, 3.0).expect("weight").weight, 3.0);
        assert!(!r.set_enabled(s.id, false).expect("enabled").enabled);
        assert!(r.set_favorite(s.id, true).expect("favorite").favorite);

        let stored = r.get(s.id).expect("get");
        assert_eq!(stored.duration_ms, Some(1400));
        assert_eq!(stored.sample_rate, Some(44_100));
        assert_eq!(stored.format, "wav");
    }

    #[test]
    fn updating_a_missing_sound_is_an_error() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        assert!(r.rename(404, "nope").is_err());
        assert!(r.delete(404).is_err());
        assert!(r.get(404).is_err());
    }

    #[test]
    fn tags_are_normalized_deduplicated_and_queryable() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);

        let wooden = r
            .import(&new_sound("Wooden", "/s/wood.wav"))
            .expect("import");
        let metal = r
            .import(&new_sound("Metal", "/s/metal.wav"))
            .expect("import");

        r.set_tags(
            wooden.id,
            &["Wood".into(), "  dungeon ".into(), "wood".into(), "".into()],
        )
        .expect("tags");
        r.set_tags(metal.id, &["metal".into(), "dungeon".into()])
            .expect("tags");

        assert_eq!(r.tags(wooden.id).expect("tags"), vec!["dungeon", "wood"]);

        let dungeon = r.by_tags(&["dungeon".into()]).expect("by tags");
        assert_eq!(dungeon.len(), 2);

        let wooden_dungeon = r
            .by_tags(&["wood".into(), "dungeon".into()])
            .expect("by tags");
        assert_eq!(wooden_dungeon.len(), 1, "must require ALL tags, not any");
        assert_eq!(wooden_dungeon[0].id, wooden.id);
    }

    #[test]
    fn setting_tags_replaces_the_previous_set() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let s = r.import(&new_sound("S", "/s/a.wav")).expect("import");

        r.set_tags(s.id, &["wood".into(), "old".into()])
            .expect("tags");
        r.set_tags(s.id, &["metal".into()]).expect("tags");

        assert_eq!(r.tags(s.id).expect("tags"), vec!["metal"]);
    }

    #[test]
    fn groups_keep_their_members_in_insertion_order() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);

        let group = r.create_group("Wooden Doors").expect("group");
        let a = r.import(&new_sound("A", "/s/a.wav")).expect("import");
        let b = r.import(&new_sound("B", "/s/b.wav")).expect("import");
        let c = r.import(&new_sound("C", "/s/c.wav")).expect("import");

        for sound in [&c, &a, &b] {
            r.add_to_group(group.id, sound.id).expect("add");
        }

        let members: Vec<i64> = r
            .group_members(group.id)
            .expect("members")
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(members, vec![c.id, a.id, b.id]);
    }

    #[test]
    fn adding_a_sound_to_a_group_twice_is_idempotent() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let group = r.create_group("G").expect("group");
        let s = r.import(&new_sound("A", "/s/a.wav")).expect("import");

        r.add_to_group(group.id, s.id).expect("add");
        r.add_to_group(group.id, s.id).expect("add again");

        assert_eq!(r.group_members(group.id).expect("members").len(), 1);
    }

    #[test]
    fn group_membership_requires_both_rows_to_exist() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let group = r.create_group("G").expect("group");
        let s = r.import(&new_sound("A", "/s/a.wav")).expect("import");

        assert!(r.add_to_group(999, s.id).is_err(), "missing group");
        assert!(r.add_to_group(group.id, 999).is_err(), "missing sound");
    }

    #[test]
    fn deleting_a_sound_removes_it_from_its_groups() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let group = r.create_group("G").expect("group");
        let s = r.import(&new_sound("A", "/s/a.wav")).expect("import");
        r.add_to_group(group.id, s.id).expect("add");

        r.delete(s.id).expect("delete");
        assert!(r.group_members(group.id).expect("members").is_empty());
    }

    #[test]
    fn deleting_a_group_leaves_the_sounds_alone() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let group = r.create_group("G").expect("group");
        let s = r.import(&new_sound("A", "/s/a.wav")).expect("import");
        r.add_to_group(group.id, s.id).expect("add");

        r.delete_group(group.id).expect("delete group");
        assert_eq!(
            r.count().expect("count"),
            1,
            "the file metadata must survive"
        );
    }

    #[test]
    fn group_settings_round_trip_and_reject_unknown_modes() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        let group = r.create_group("Doors").expect("group");

        assert_eq!(group.selection_mode, "random");
        assert!(group.prevent_repeat, "anti-repeat defaults on");

        let updated = r
            .update_group(group.id, "Old Doors", "weighted", false, 0.85)
            .expect("update");
        assert_eq!(updated.name, "Old Doors");
        assert_eq!(updated.selection_mode, "weighted");
        assert!(!updated.prevent_repeat);
        assert_eq!(updated.volume, 0.85);

        assert!(r
            .update_group(group.id, "X", "telepathy", true, 1.0)
            .is_err());
    }

    #[test]
    fn group_names_are_unique() {
        let db = Db::open_in_memory().expect("open");
        let r = repo(&db);
        r.create_group("Doors").expect("group");
        assert!(r.create_group("Doors").is_err());
        assert!(r.group_by_name("Doors").expect("lookup").is_some());
    }
}
