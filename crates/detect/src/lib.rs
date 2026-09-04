//! The event detection engine.
//!
//! Input: a transcript. Output: scored candidate events, each with the reasoning behind
//! its score. This crate is pure — no filesystem, no audio, no database, no Tauri — so
//! the corpus of realistic narration that guards against false positives runs as a
//! plain, fast `cargo test`.
//!
//! It also never plays a sound. It returns decisions; the sound engine acts on them.

pub mod corpus;

mod engine;
mod event;
mod fuzzy;
mod normalize;
mod seed;
mod stem;
mod trigger;

pub use engine::{
    Candidate, Detection, DetectionInput, Detector, MatchLayer, RejectionReason, SemanticScorer,
};
pub use event::{terms, EventDefinition, EventKind, Lang, Phrase, Term, TermKind};
pub use normalize::{normalize, normalize_text, Normalized};
pub use seed::seed_events;
pub use stem::{stem, stem_phrase};
pub use trigger::{
    AlwaysRoll, Decision, Roll, Suppressed, SuppressionReason, Trigger, TriggerEngine, TriggerRules,
};
