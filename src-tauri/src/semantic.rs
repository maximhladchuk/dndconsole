//! Wiring the semantic layer into the application.
//!
//! Optional by design: the embedding model is a 113 MB download, and everything works
//! without it — just with less reach. When it is present, phrase vectors are cached in
//! the database so startup does not pay for them twice.

use std::sync::{Arc, Mutex};

use dndsound_models::ModelStore;
use dndsound_semantic::{Embedder, EmbedderConfig, EmbeddingCache, SemanticEventIndex};
use dndsound_store::Db;

/// Catalog ids of the two files the semantic layer needs.
pub const EMBEDDING_MODEL_ID: &str = "multilingual-e5-small-int8";
pub const EMBEDDING_TOKENIZER_ID: &str = "multilingual-e5-small-tokenizer";

/// Load the embedder, or `None` when the model has not been downloaded.
pub fn load_embedder(models: &ModelStore) -> Option<Arc<Embedder>> {
    let model = models.require(EMBEDDING_MODEL_ID).ok()?;
    let tokenizer = models.require(EMBEDDING_TOKENIZER_ID).ok()?;

    match Embedder::load(model, tokenizer, EmbedderConfig::default()) {
        Ok(embedder) => {
            tracing::info!("semantic matching is available");
            Some(Arc::new(embedder))
        }
        Err(e) => {
            // A broken model must not stop the application starting; the other detection
            // layers are unaffected.
            tracing::error!(error = %e, "the embedding model failed to load; semantic matching is off");
            None
        }
    }
}

/// The phrase-embedding cache, backed by SQLite.
pub struct StoreCache {
    db: Arc<Mutex<Db>>,
    model_id: String,
}

impl StoreCache {
    pub fn new(db: Arc<Mutex<Db>>) -> Self {
        Self {
            db,
            model_id: EMBEDDING_MODEL_ID.to_string(),
        }
    }
}

impl EmbeddingCache for StoreCache {
    fn get(&self, _event_id: &str, phrase_text: &str) -> Option<Vec<f32>> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        db.embeddings()
            .get(
                &self.model_id,
                &dndsound_store::embeddings::phrase_hash(phrase_text),
            )
            .ok()
            .flatten()
    }

    fn put(&self, event_id: &str, phrase_text: &str, vector: &[f32]) {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());

        // The vector is tied to the phrase row so deleting a phrase drops its vector.
        let phrase_id: Option<i64> = db
            .conn()
            .query_row(
                "SELECT id FROM event_phrases WHERE event_id = ?1 AND text = ?2 LIMIT 1",
                rusqlite::params![event_id, phrase_text],
                |row| row.get(0),
            )
            .ok();

        let Some(phrase_id) = phrase_id else {
            // A phrase that is not in the database yet (a simulation, say) is simply not
            // cached; recomputing it costs milliseconds.
            return;
        };

        if let Err(e) = db.embeddings().put(
            &self.model_id,
            &dndsound_store::embeddings::phrase_hash(phrase_text),
            phrase_id,
            vector,
        ) {
            tracing::warn!(error = %e, "could not cache a phrase embedding");
        }
    }
}

/// Build the semantic index for the events currently in the database.
pub fn build_index(
    embedder: Arc<Embedder>,
    db: Arc<Mutex<Db>>,
    events: &[dndsound_detect::EventDefinition],
) -> Option<Arc<SemanticEventIndex>> {
    let started = std::time::Instant::now();
    let cache = StoreCache::new(db);

    match SemanticEventIndex::build_cached(embedder, events, &cache) {
        Ok(index) => {
            tracing::info!(
                phrases = index.phrase_count(),
                ms = started.elapsed().as_millis(),
                "semantic index built"
            );
            Some(Arc::new(index))
        }
        Err(e) => {
            tracing::error!(error = %e, "could not build the semantic index");
            None
        }
    }
}
