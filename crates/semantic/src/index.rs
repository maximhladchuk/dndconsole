//! A small vector index over event example phrases.
//!
//! Every phrase of every event is embedded once, at load time, and kept in one flat
//! matrix. A transcript is embedded once and compared against all of them — a few
//! million multiply-adds, far cheaper than the tokenizer that produced the vector.
//!
//! The design target is 5,000 events. Ten phrases each is 50,000 vectors of
//! 384 floats: 77 MB and one pass. That is still the cheap half of detection, so there
//! is no index structure here beyond a flat scan — an approximate index would add
//! failure modes to save time we are not spending.

use std::collections::HashMap;
use std::sync::Arc;

use dndsound_detect::{EventDefinition, SemanticScorer};

use crate::embed::{cosine, Embedder, Role};
use crate::Result;

pub struct SemanticEventIndex {
    embedder: Arc<Embedder>,
    /// One entry per phrase: which event it belongs to, and its vector.
    entries: Vec<(String, Vec<f32>)>,
}

/// Somewhere to keep phrase vectors between runs.
///
/// Embedding a phrase costs a few milliseconds, which is nothing for five events and
/// two and a half minutes for the five thousand events the architecture has to
/// support. Implemented over SQLite by the application; `NoCache` is for tests.
pub trait EmbeddingCache {
    fn get(&self, event_id: &str, phrase_text: &str) -> Option<Vec<f32>>;
    fn put(&self, event_id: &str, phrase_text: &str, vector: &[f32]);
}

/// Recompute every time. Fine for a handful of events.
pub struct NoCache;

impl EmbeddingCache for NoCache {
    fn get(&self, _event_id: &str, _phrase_text: &str) -> Option<Vec<f32>> {
        None
    }
    fn put(&self, _event_id: &str, _phrase_text: &str, _vector: &[f32]) {}
}

impl SemanticEventIndex {
    /// Embed every example phrase of every enabled event.
    pub fn build(embedder: Arc<Embedder>, events: &[EventDefinition]) -> Result<Self> {
        Self::build_cached(embedder, events, &NoCache)
    }

    /// Build, reusing cached vectors and computing only what is missing.
    pub fn build_cached(
        embedder: Arc<Embedder>,
        events: &[EventDefinition],
        cache: &dyn EmbeddingCache,
    ) -> Result<Self> {
        let mut entries: Vec<(String, Vec<f32>)> = Vec::new();

        // Everything the cache does not have, embedded in one batch rather than one
        // call per phrase.
        let mut missing_owners: Vec<String> = Vec::new();
        let mut missing_texts: Vec<&str> = Vec::new();

        for event in events.iter().filter(|event| event.enabled) {
            for phrase in event.examples() {
                match cache.get(&event.id, &phrase.text) {
                    Some(vector) => entries.push((event.id.clone(), vector)),
                    None => {
                        missing_owners.push(event.id.clone());
                        missing_texts.push(phrase.text.as_str());
                    }
                }
            }
        }

        if !missing_texts.is_empty() {
            let vectors = embedder.embed_batch(&missing_texts, Role::Passage)?;
            for ((event_id, text), vector) in
                missing_owners.into_iter().zip(missing_texts).zip(vectors)
            {
                cache.put(&event_id, text, &vector);
                entries.push((event_id, vector));
            }
        }

        Ok(Self { embedder, entries })
    }

    pub fn phrase_count(&self) -> usize {
        self.entries.len()
    }

    /// Best similarity per event, strongest first.
    pub fn similar(&self, transcript: &str) -> Result<Vec<(String, f32)>> {
        if self.entries.is_empty() {
            return Ok(Vec::new());
        }

        let query = self.embedder.embed(transcript, Role::Query)?;

        // An event is as similar as its closest phrasing; averaging would punish events
        // that cover many different situations.
        let mut best: HashMap<&str, f32> = HashMap::new();
        for (event_id, vector) in &self.entries {
            let score = cosine(&query, vector);
            let entry = best.entry(event_id.as_str()).or_insert(f32::MIN);
            if score > *entry {
                *entry = score;
            }
        }

        let mut scores: Vec<(String, f32)> = best
            .into_iter()
            .map(|(id, score)| (id.to_string(), score))
            .collect();
        scores.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(scores)
    }
}

impl SemanticScorer for SemanticEventIndex {
    fn similar_events(&self, transcript: &str) -> Vec<(String, f32)> {
        match self.similar(transcript) {
            Ok(scores) => scores,
            Err(e) => {
                // A failed embedding must degrade to "no semantic opinion", never take
                // the whole detector down mid-session.
                tracing::error!(error = %e, "semantic scoring failed; falling back to the other layers");
                Vec::new()
            }
        }
    }
}
