//! Event definitions: reading and writing the rules that turn narration into sounds.
//!
//! The detector works with `dndsound_detect::EventDefinition`; the database stores it
//! across four tables. This module is the only place that knows both shapes.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{now_ms, Error, Result};

/// An event plus the things the detector does not need but the UI does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEvent {
    pub definition: dndsound_detect::EventDefinition,
    /// Sound group this event plays from, if one is assigned.
    pub sound_group_id: Option<i64>,
    /// Which mixer bus it plays on.
    pub track: String,
    /// True when this event ships with the application.
    pub builtin: bool,
    /// True once a person has edited it, which stops the seed overwriting it and lets
    /// the editor offer to reset it.
    pub user_modified: bool,
}

pub struct EventsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EventsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert the events the app ships with, if the table is empty.
    ///
    /// Only on an empty table: re-seeding would resurrect events the user deleted on
    /// purpose, which is worse than shipping nothing.
    pub fn seed_if_empty(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        if count > 0 {
            return Ok(0);
        }

        let events = dndsound_detect::seed_events();
        for event in &events {
            self.upsert(event, None, "sfx")?;
            self.mark_builtin(&event.id)?;
        }
        tracing::info!(count = events.len(), "seeded default events");
        Ok(events.len())
    }

    /// Bring built-in events up to date with the code, without touching anything a
    /// person has edited.
    ///
    /// Run on every startup. Three cases:
    ///
    /// * A built-in event that is not in the database yet is added — this is how new
    ///   events reach an existing installation.
    /// * A built-in event nobody has edited is replaced with the current definition,
    ///   keeping whichever sound group it already points at.
    /// * Anything the user has edited, and anything the user created, is left exactly
    ///   as it is. Their work outranks ours.
    ///
    /// Returns how many events were written.
    pub fn sync_builtin(&self) -> Result<usize> {
        let mut written = 0;

        for definition in dndsound_detect::seed_events() {
            let existing: Option<(i64, Option<i64>, String)> = self
                .conn
                .query_row(
                    "SELECT user_modified, sound_group_id, track FROM events WHERE id = ?1",
                    [&definition.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;

            match existing {
                Some((modified, _, _)) if modified != 0 => continue,
                Some((_, sound_group_id, track)) => {
                    self.upsert(&definition, sound_group_id, &track)?;
                }
                None => self.upsert(&definition, None, "sfx")?,
            }

            self.mark_builtin(&definition.id)?;
            written += 1;
        }

        if written > 0 {
            tracing::info!(count = written, "refreshed built-in events from the seed");
        }
        Ok(written)
    }

    fn mark_builtin(&self, id: &str) -> Result<()> {
        self.conn
            .execute("UPDATE events SET builtin = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Record that a person has changed this event, so the seed stops overwriting it.
    pub fn mark_user_modified(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE events SET user_modified = 1, updated_at = ?2 WHERE id = ?1",
            params![id, now_ms()],
        )?;
        Ok(())
    }

    /// Forget a user's edits to a built-in event and take the current definition.
    pub fn reset_to_builtin(&self, id: &str) -> Result<()> {
        let Some(definition) = dndsound_detect::seed_events()
            .into_iter()
            .find(|e| e.id == id)
        else {
            return Err(Error::NotFound(format!("built-in event {id}")));
        };

        let (sound_group_id, track): (Option<i64>, String) = self
            .conn
            .query_row(
                "SELECT sound_group_id, track FROM events WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .unwrap_or((None, "sfx".to_string()));

        self.upsert(&definition, sound_group_id, &track)?;
        self.conn.execute(
            "UPDATE events SET builtin = 1, user_modified = 0 WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, category, kind, track, sound_group_id,
                    confidence_threshold, cooldown_ms, probability, require_action_word,
                    enabled, builtin, user_modified
             FROM events ORDER BY category, id",
        )?;

        let rows: Vec<StoredEvent> = stmt
            .query_map([], map_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Phrases and terms are fetched per event rather than in one join: the event
        // count is small, and the join would need de-duplication anyway.
        rows.into_iter()
            .map(|mut event| {
                event.definition.phrases = self.phrases_of(&event.definition.id)?;
                event.definition.terms = self.terms_of(&event.definition.id)?;
                Ok(event)
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<StoredEvent> {
        let mut event = self
            .conn
            .query_row(
                "SELECT id, display_name, category, kind, track, sound_group_id,
                        confidence_threshold, cooldown_ms, probability, require_action_word,
                        enabled, builtin, user_modified
                 FROM events WHERE id = ?1",
                [id],
                map_event,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("event {id}")))?;

        event.definition.phrases = self.phrases_of(id)?;
        event.definition.terms = self.terms_of(id)?;
        Ok(event)
    }

    /// Create or replace an event and all of its phrases and terms.
    pub fn upsert(
        &self,
        definition: &dndsound_detect::EventDefinition,
        sound_group_id: Option<i64>,
        track: &str,
    ) -> Result<()> {
        let now = now_ms();

        self.conn.execute(
            "INSERT INTO events
                 (id, display_name, category, sound_group_id, kind, track,
                  confidence_threshold, cooldown_ms, probability, require_action_word,
                  enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
             ON CONFLICT(id) DO UPDATE SET
                 display_name         = excluded.display_name,
                 category             = excluded.category,
                 sound_group_id       = excluded.sound_group_id,
                 kind                 = excluded.kind,
                 track                = excluded.track,
                 confidence_threshold = excluded.confidence_threshold,
                 cooldown_ms          = excluded.cooldown_ms,
                 probability          = excluded.probability,
                 require_action_word  = excluded.require_action_word,
                 enabled              = excluded.enabled,
                 updated_at           = excluded.updated_at",
            params![
                definition.id,
                definition.display_name,
                definition.category,
                sound_group_id,
                kind_to_text(definition.kind),
                track,
                definition.confidence_threshold,
                definition.cooldown_ms,
                definition.probability,
                definition.require_action_word as i64,
                definition.enabled as i64,
                now,
            ],
        )?;

        // Phrases and terms are replaced wholesale: the editor sends the full set, and
        // diffing them would only add ways to get out of sync.
        self.conn.execute(
            "DELETE FROM event_phrases WHERE event_id = ?1",
            [&definition.id],
        )?;
        self.conn.execute(
            "DELETE FROM event_terms WHERE event_id = ?1",
            [&definition.id],
        )?;

        for phrase in &definition.phrases {
            self.conn.execute(
                "INSERT INTO event_phrases (event_id, lang, text, kind, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT DO NOTHING",
                params![
                    definition.id,
                    phrase.lang.as_str(),
                    phrase.text,
                    if phrase.is_command {
                        "command"
                    } else {
                        "example"
                    },
                    now
                ],
            )?;
        }

        for term in &definition.terms {
            self.conn.execute(
                "INSERT INTO event_terms (event_id, lang, text, kind)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT DO NOTHING",
                params![
                    definition.id,
                    term.lang.as_str(),
                    term.text,
                    term_kind_to_text(term.kind)
                ],
            )?;
        }

        Ok(())
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE events SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, enabled as i64, now_ms()],
        )?;
        if changed == 0 {
            return Err(Error::NotFound(format!("event {id}")));
        }
        Ok(())
    }

    pub fn set_sound_group(&self, id: &str, sound_group_id: Option<i64>) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE events SET sound_group_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, sound_group_id, now_ms()],
        )?;
        if changed == 0 {
            return Err(Error::NotFound(format!("event {id}")));
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM events WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(Error::NotFound(format!("event {id}")));
        }
        Ok(())
    }

    fn phrases_of(&self, event_id: &str) -> Result<Vec<dndsound_detect::Phrase>> {
        let mut stmt = self.conn.prepare(
            "SELECT lang, text, kind FROM event_phrases WHERE event_id = ?1 ORDER BY id",
        )?;

        let rows = stmt.query_map([event_id], |row| {
            let lang: String = row.get(0)?;
            let text: String = row.get(1)?;
            let kind: String = row.get(2)?;
            Ok(dndsound_detect::Phrase {
                lang: dndsound_detect::Lang::parse(&lang),
                text,
                is_command: kind == "command",
            })
        })?;

        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn terms_of(&self, event_id: &str) -> Result<Vec<dndsound_detect::Term>> {
        let mut stmt = self
            .conn
            .prepare("SELECT lang, text, kind FROM event_terms WHERE event_id = ?1 ORDER BY id")?;

        let rows = stmt.query_map([event_id], |row| {
            let lang: String = row.get(0)?;
            let text: String = row.get(1)?;
            let kind: String = row.get(2)?;
            Ok(dndsound_detect::Term {
                kind: text_to_term_kind(&kind),
                lang: dndsound_detect::Lang::parse(&lang),
                text,
            })
        })?;

        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredEvent> {
    let kind: String = row.get(3)?;
    Ok(StoredEvent {
        definition: dndsound_detect::EventDefinition {
            id: row.get(0)?,
            display_name: row.get(1)?,
            category: row.get(2)?,
            kind: text_to_kind(&kind),
            phrases: Vec::new(),
            terms: Vec::new(),
            confidence_threshold: row.get(6)?,
            cooldown_ms: row.get(7)?,
            probability: row.get(8)?,
            require_action_word: row.get::<_, i64>(9)? != 0,
            enabled: row.get::<_, i64>(10)? != 0,
        },
        track: row.get(4)?,
        sound_group_id: row.get(5)?,
        builtin: row.get::<_, i64>(11)? != 0,
        user_modified: row.get::<_, i64>(12)? != 0,
    })
}

fn kind_to_text(kind: dndsound_detect::EventKind) -> &'static str {
    match kind {
        dndsound_detect::EventKind::OneShot => "one_shot",
        dndsound_detect::EventKind::AmbienceStart => "ambience_start",
        dndsound_detect::EventKind::AmbienceStop => "ambience_stop",
    }
}

fn text_to_kind(text: &str) -> dndsound_detect::EventKind {
    match text {
        "ambience_start" => dndsound_detect::EventKind::AmbienceStart,
        "ambience_stop" => dndsound_detect::EventKind::AmbienceStop,
        _ => dndsound_detect::EventKind::OneShot,
    }
}

fn term_kind_to_text(kind: dndsound_detect::TermKind) -> &'static str {
    match kind {
        dndsound_detect::TermKind::Keyword => "keyword",
        dndsound_detect::TermKind::Action => "action",
        dndsound_detect::TermKind::Negative => "negative",
    }
}

fn text_to_term_kind(text: &str) -> dndsound_detect::TermKind {
    match text {
        "action" => dndsound_detect::TermKind::Action,
        "negative" => dndsound_detect::TermKind::Negative,
        _ => dndsound_detect::TermKind::Keyword,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;
    use dndsound_detect::{EventDefinition, Lang, Phrase, Term, TermKind};

    fn repo(db: &Db) -> EventsRepo<'_> {
        EventsRepo::new(db.conn())
    }

    #[test]
    fn seeding_populates_an_empty_database_once() {
        let db = Db::open_in_memory().expect("db");

        let first = repo(&db).seed_if_empty().expect("seed");
        assert!(first >= 5, "expected the seed events");

        let second = repo(&db).seed_if_empty().expect("seed again");
        assert_eq!(second, 0, "seeding twice must not duplicate events");
        assert_eq!(repo(&db).list().expect("list").len(), first);
    }

    #[test]
    fn seeded_events_round_trip_with_their_phrases_and_terms() {
        let db = Db::open_in_memory().expect("db");
        repo(&db).seed_if_empty().expect("seed");

        let stored = repo(&db).get("OPEN_DOOR").expect("get");
        let seeded = dndsound_detect::seed_events()
            .into_iter()
            .find(|event| event.id == "OPEN_DOOR")
            .expect("seed contains OPEN_DOOR");

        assert_eq!(stored.definition.display_name, seeded.display_name);
        assert_eq!(stored.definition.phrases.len(), seeded.phrases.len());
        assert_eq!(stored.definition.terms.len(), seeded.terms.len());
        assert_eq!(stored.definition.cooldown_ms, seeded.cooldown_ms);
        assert!(stored.definition.require_action_word);
    }

    #[test]
    fn the_stored_events_still_detect_correctly() {
        // The real risk in this module is losing a term or a language on the way through
        // SQLite, which would quietly weaken detection.
        let db = Db::open_in_memory().expect("db");
        repo(&db).seed_if_empty().expect("seed");

        let definitions: Vec<EventDefinition> = repo(&db)
            .list()
            .expect("list")
            .into_iter()
            .map(|stored| stored.definition)
            .collect();

        let detector = dndsound_detect::Detector::new(definitions);

        let fired = detector
            .detect(dndsound_detect::DetectionInput::final_transcript(
                "Він різко відчиняє двері.",
                0,
            ))
            .best()
            .map(|c| c.event_id.clone());
        assert_eq!(fired.as_deref(), Some("OPEN_DOOR"));

        let silent = detector
            .detect(dndsound_detect::DetectionInput::final_transcript(
                "A sword hangs on the wall.",
                0,
            ))
            .best()
            .map(|c| c.event_id.clone());
        assert_eq!(silent, None);
    }

    #[test]
    fn upsert_replaces_phrases_rather_than_accumulating_them() {
        let db = Db::open_in_memory().expect("db");
        let r = repo(&db);

        let mut event = EventDefinition::new("TEST", "Test")
            .with_phrases(vec![
                Phrase::example(Lang::En, "one"),
                Phrase::example(Lang::En, "two"),
            ])
            .with_terms(vec![Term::keyword("thing"), Term::action("does")]);
        r.upsert(&event, None, "sfx").expect("insert");

        event.phrases = vec![Phrase::example(Lang::Uk, "три")];
        event.terms = vec![Term::negative("no")];
        r.upsert(&event, None, "sfx").expect("update");

        let stored = r.get("TEST").expect("get");
        assert_eq!(stored.definition.phrases.len(), 1);
        assert_eq!(stored.definition.phrases[0].lang, Lang::Uk);
        assert_eq!(stored.definition.terms.len(), 1);
        assert_eq!(stored.definition.terms[0].kind, TermKind::Negative);
    }

    #[test]
    fn commands_survive_the_round_trip_as_commands() {
        let db = Db::open_in_memory().expect("db");
        let r = repo(&db);

        let event = EventDefinition::new("TEST", "Test").with_phrases(vec![
            Phrase::example(Lang::En, "the thunder rolls"),
            Phrase::command("sound thunder"),
        ]);
        r.upsert(&event, None, "sfx").expect("insert");

        let stored = r.get("TEST").expect("get");
        assert_eq!(stored.definition.commands().count(), 1);
        assert_eq!(stored.definition.examples().count(), 1);
    }

    #[test]
    fn sound_group_and_enabled_state_can_be_changed_independently() {
        let db = Db::open_in_memory().expect("db");
        let r = repo(&db);
        let group = db.sounds().create_group("Doors").expect("group");
        r.upsert(&EventDefinition::new("TEST", "Test"), None, "sfx")
            .expect("insert");

        r.set_sound_group("TEST", Some(group.id)).expect("assign");
        assert_eq!(r.get("TEST").expect("get").sound_group_id, Some(group.id));

        r.set_enabled("TEST", false).expect("disable");
        assert!(!r.get("TEST").expect("get").definition.enabled);
    }

    #[test]
    fn deleting_an_event_takes_its_phrases_with_it() {
        let db = Db::open_in_memory().expect("db");
        let r = repo(&db);
        r.upsert(
            &EventDefinition::new("TEST", "Test")
                .with_phrases(vec![Phrase::example(Lang::En, "one")]),
            None,
            "sfx",
        )
        .expect("insert");

        r.delete("TEST").expect("delete");

        let orphans: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM event_phrases WHERE event_id = 'TEST'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(orphans, 0);
        assert!(r.get("TEST").is_err());
    }

    #[test]
    fn missing_events_are_reported_rather_than_silently_ignored() {
        let db = Db::open_in_memory().expect("db");
        let r = repo(&db);
        assert!(r.get("NOPE").is_err());
        assert!(r.delete("NOPE").is_err());
        assert!(r.set_enabled("NOPE", true).is_err());
        assert!(r.set_sound_group("NOPE", None).is_err());
    }
}

#[cfg(test)]
mod seed_sync_tests {
    use crate::Db;

    fn db() -> Db {
        Db::open_in_memory().expect("db")
    }

    #[test]
    fn syncing_adds_events_that_did_not_exist_yet() {
        let db = db();
        // A database seeded before an event existed: seed everything, then drop one.
        db.events().seed_if_empty().expect("seed");
        db.events().delete("THUNDER").expect("delete");
        assert!(db.events().get("THUNDER").is_err());

        let written = db.events().sync_builtin().expect("sync");
        assert!(written >= 1);
        assert!(
            db.events().get("THUNDER").is_ok(),
            "a new built-in must arrive on upgrade"
        );
    }

    #[test]
    fn syncing_refreshes_an_untouched_builtin() {
        let db = db();
        db.events().seed_if_empty().expect("seed");

        // Simulate an old database whose terms predate a fix: strip them all.
        db.conn()
            .execute("DELETE FROM event_terms WHERE event_id = 'SWORD_SWING'", [])
            .expect("strip");

        db.events().sync_builtin().expect("sync");

        let terms: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM event_terms WHERE event_id = 'SWORD_SWING'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert!(
            terms > 0,
            "an untouched built-in must be brought up to date"
        );
    }

    #[test]
    fn syncing_never_overwrites_an_event_a_person_edited() {
        let db = db();
        db.events().seed_if_empty().expect("seed");

        let mut edited = db.events().get("OPEN_DOOR").expect("get").definition;
        edited.phrases = vec![dndsound_detect::Phrase::example(
            dndsound_detect::Lang::Uk,
            "власна фраза користувача",
        )];
        edited.terms = vec![dndsound_detect::Term::keyword("власне")];
        db.events().upsert(&edited, None, "sfx").expect("save");
        db.events().mark_user_modified("OPEN_DOOR").expect("mark");

        db.events().sync_builtin().expect("sync");

        let after = db.events().get("OPEN_DOOR").expect("get");
        assert!(after.user_modified);
        assert_eq!(
            after.definition.phrases.len(),
            1,
            "the user's phrases were replaced"
        );
        assert_eq!(after.definition.phrases[0].text, "власна фраза користувача");
    }

    #[test]
    fn syncing_keeps_the_sound_group_an_event_already_points_at() {
        let db = db();
        db.events().seed_if_empty().expect("seed");
        let group = db.sounds().create_group("Doors").expect("group");
        db.events()
            .set_sound_group("OPEN_DOOR", Some(group.id))
            .expect("assign");

        db.events().sync_builtin().expect("sync");

        assert_eq!(
            db.events().get("OPEN_DOOR").expect("get").sound_group_id,
            Some(group.id),
            "refreshing the definition must not unhook the sounds"
        );
    }

    #[test]
    fn resetting_takes_the_builtin_definition_back() {
        let db = db();
        db.events().seed_if_empty().expect("seed");

        let mut edited = db.events().get("WOLF_GROWL").expect("get").definition;
        edited.phrases.clear();
        edited.terms.clear();
        db.events().upsert(&edited, None, "sfx").expect("save");
        db.events().mark_user_modified("WOLF_GROWL").expect("mark");

        db.events().reset_to_builtin("WOLF_GROWL").expect("reset");

        let after = db.events().get("WOLF_GROWL").expect("get");
        assert!(!after.user_modified);
        assert!(after.builtin);
        assert!(
            !after.definition.phrases.is_empty(),
            "the seed phrasing must come back"
        );
    }

    #[test]
    fn a_user_created_event_is_never_touched_by_the_sync() {
        let db = db();
        db.events().seed_if_empty().expect("seed");

        let mine = dndsound_detect::EventDefinition::new("MY_EVENT", "Mine").with_phrases(vec![
            dndsound_detect::Phrase::example(dndsound_detect::Lang::Uk, "моя фраза"),
        ]);
        db.events().upsert(&mine, None, "sfx").expect("save");

        db.events().sync_builtin().expect("sync");

        let after = db.events().get("MY_EVENT").expect("still there");
        assert!(!after.builtin);
        assert_eq!(after.definition.phrases.len(), 1);
    }
}
