//! Cached phrase embeddings.
//!
//! Embedding one phrase takes about three milliseconds. Five events is nothing; the
//! spec's target of 5,000 events with ten phrases each is 50,000 embeddings, which is
//! two and a half minutes of startup. So they are computed once and kept, keyed by the
//! phrase text and the model that produced them — change either and the entry is simply
//! missed and recomputed.

use rusqlite::{params, Connection};

use crate::{now_ms, Result};

pub struct EmbeddingsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EmbeddingsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Look up a cached vector by phrase hash.
    pub fn get(&self, model_id: &str, phrase_hash: &str) -> Result<Option<Vec<f32>>> {
        let mut stmt = self.conn.prepare(
            "SELECT vector FROM phrase_embeddings WHERE model_id = ?1 AND phrase_hash = ?2 LIMIT 1",
        )?;

        let mut rows = stmt.query(params![model_id, phrase_hash])?;
        match rows.next()? {
            Some(row) => {
                let blob: Vec<u8> = row.get(0)?;
                Ok(Some(decode(&blob)))
            }
            None => Ok(None),
        }
    }

    /// Store a vector. `phrase_id` links it to the phrase row when there is one.
    pub fn put(
        &self,
        model_id: &str,
        phrase_hash: &str,
        phrase_id: i64,
        vector: &[f32],
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO phrase_embeddings (phrase_id, model_id, phrase_hash, dim, vector, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(phrase_id, model_id) DO UPDATE SET
                 phrase_hash = excluded.phrase_hash,
                 dim         = excluded.dim,
                 vector      = excluded.vector,
                 created_at  = excluded.created_at",
            params![
                phrase_id,
                model_id,
                phrase_hash,
                vector.len() as i64,
                encode(vector),
                now_ms()
            ],
        )?;
        Ok(())
    }

    /// Drop everything for a model, e.g. after it is deleted or replaced.
    pub fn clear_model(&self, model_id: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM phrase_embeddings WHERE model_id = ?1",
            [model_id],
        )?)
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM phrase_embeddings", [], |row| {
                row.get(0)
            })?)
    }
}

/// Vectors are stored as little-endian f32, which is compact and exactly reversible.
fn encode(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// A stable hash of a phrase, used as the cache key.
///
/// FNV-1a: not cryptographic, and does not need to be — a collision would reuse the
/// wrong vector for one phrase, and the phrase text is right there to change.
pub fn phrase_hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    fn phrase_row(db: &Db) -> i64 {
        db.events()
            .upsert(
                &dndsound_detect::EventDefinition::new("TEST", "Test").with_phrases(vec![
                    dndsound_detect::Phrase::example(dndsound_detect::Lang::En, "opens the door"),
                ]),
                None,
                "sfx",
            )
            .expect("event");

        db.conn()
            .query_row("SELECT id FROM event_phrases LIMIT 1", [], |row| row.get(0))
            .expect("phrase id")
    }

    #[test]
    fn vectors_round_trip_exactly() {
        let db = Db::open_in_memory().expect("db");
        let phrase_id = phrase_row(&db);
        let repo = EmbeddingsRepo::new(db.conn());

        let vector: Vec<f32> = (0..384).map(|i| i as f32 / 384.0 - 0.5).collect();
        repo.put("e5-small", "abc123", phrase_id, &vector)
            .expect("put");

        let restored = repo
            .get("e5-small", "abc123")
            .expect("get")
            .expect("present");
        assert_eq!(restored, vector, "f32 values must survive byte-exactly");
    }

    #[test]
    fn a_different_model_or_phrase_misses_the_cache() {
        let db = Db::open_in_memory().expect("db");
        let phrase_id = phrase_row(&db);
        let repo = EmbeddingsRepo::new(db.conn());

        repo.put("e5-small", "abc123", phrase_id, &[1.0, 2.0])
            .expect("put");

        assert!(repo.get("other-model", "abc123").expect("get").is_none());
        assert!(repo.get("e5-small", "different").expect("get").is_none());
    }

    #[test]
    fn re_storing_a_phrase_replaces_rather_than_duplicates() {
        let db = Db::open_in_memory().expect("db");
        let phrase_id = phrase_row(&db);
        let repo = EmbeddingsRepo::new(db.conn());

        repo.put("e5", "hash-one", phrase_id, &[1.0]).expect("put");
        repo.put("e5", "hash-two", phrase_id, &[2.0, 3.0])
            .expect("put");

        assert_eq!(repo.count().expect("count"), 1);
        assert_eq!(
            repo.get("e5", "hash-two").expect("get"),
            Some(vec![2.0, 3.0])
        );
        assert!(repo.get("e5", "hash-one").expect("get").is_none());
    }

    #[test]
    fn clearing_a_model_leaves_other_models_alone() {
        let db = Db::open_in_memory().expect("db");
        let phrase_id = phrase_row(&db);
        let repo = EmbeddingsRepo::new(db.conn());

        repo.put("old-model", "h", phrase_id, &[1.0]).expect("put");
        assert_eq!(repo.clear_model("new-model").expect("clear"), 0);
        assert_eq!(repo.clear_model("old-model").expect("clear"), 1);
        assert_eq!(repo.count().expect("count"), 0);
    }

    #[test]
    fn deleting_a_phrase_takes_its_embedding_with_it() {
        let db = Db::open_in_memory().expect("db");
        let phrase_id = phrase_row(&db);
        EmbeddingsRepo::new(db.conn())
            .put("e5", "h", phrase_id, &[1.0])
            .expect("put");

        db.events().delete("TEST").expect("delete event");
        assert_eq!(EmbeddingsRepo::new(db.conn()).count().expect("count"), 0);
    }

    #[test]
    fn hashing_is_stable_and_distinguishes_phrases() {
        assert_eq!(phrase_hash("opens the door"), phrase_hash("opens the door"));
        assert_ne!(
            phrase_hash("opens the door"),
            phrase_hash("opens the doors")
        );
        assert_ne!(phrase_hash("відчиняє двері"), phrase_hash("відчиняє вікно"));
        assert_eq!(phrase_hash("x").len(), 16);
    }
}
