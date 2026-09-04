//! What an event is.
//!
//! The abstraction the whole product rests on: narration maps to a *semantic event*,
//! and the event points at a sound group. Nothing here knows about audio files.

use serde::{Deserialize, Serialize};

/// Language tag for a phrase or term. `Any` matches regardless of the spoken language,
/// which is what makes code-switched narration work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Uk,
    Any,
}

impl Lang {
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Uk => "uk",
            Lang::Any => "any",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "en" => Lang::En,
            "uk" => Lang::Uk,
            _ => Lang::Any,
        }
    }
}

/// An example of how this event might be narrated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Phrase {
    pub lang: Lang,
    pub text: String,
    /// Commands are spoken deliberately ("sound thunder") and take priority over
    /// automatic detection.
    pub is_command: bool,
}

impl Phrase {
    pub fn example(lang: Lang, text: &str) -> Self {
        Self {
            lang,
            text: text.to_string(),
            is_command: false,
        }
    }

    pub fn command(text: &str) -> Self {
        Self {
            lang: Lang::Any,
            text: text.to_string(),
            is_command: true,
        }
    }
}

/// What a term does for an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermKind {
    /// Anchors the event in the retrieval index. Usually the object: door, sword, wolf.
    Keyword,
    /// The action that has to be happening. This is what separates "a sword lies on the
    /// table" from "he swings his sword".
    Action,
    /// If this appears, the event is suppressed outright.
    Negative,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Term {
    pub kind: TermKind,
    pub lang: Lang,
    pub text: String,
}

impl Term {
    pub fn keyword(text: &str) -> Self {
        Self {
            kind: TermKind::Keyword,
            lang: Lang::Any,
            text: text.to_string(),
        }
    }
    pub fn action(text: &str) -> Self {
        Self {
            kind: TermKind::Action,
            lang: Lang::Any,
            text: text.to_string(),
        }
    }
    pub fn negative(text: &str) -> Self {
        Self {
            kind: TermKind::Negative,
            lang: Lang::Any,
            text: text.to_string(),
        }
    }

    /// Many terms of one kind at once.
    ///
    /// Coverage is the whole game for these lists — a verb the narrator uses and the
    /// event does not list is a sound that never plays — and writing a hundred
    /// `Term::action(...)` calls per event buries the words in punctuation. These
    /// helpers keep a term list readable enough to actually audit.
    pub fn keywords(texts: &[&str]) -> Vec<Term> {
        texts.iter().map(|t| Term::keyword(t)).collect()
    }

    pub fn actions(texts: &[&str]) -> Vec<Term> {
        texts.iter().map(|t| Term::action(t)).collect()
    }

    pub fn negatives(texts: &[&str]) -> Vec<Term> {
        texts.iter().map(|t| Term::negative(t)).collect()
    }
}

/// Collect several term lists into one.
pub fn terms(groups: impl IntoIterator<Item = Vec<Term>>) -> Vec<Term> {
    groups.into_iter().flatten().collect()
}

/// How an event behaves when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    OneShot,
    AmbienceStart,
    AmbienceStop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDefinition {
    /// Semantic identifier, e.g. `OPEN_DOOR`.
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub kind: EventKind,

    pub phrases: Vec<Phrase>,
    pub terms: Vec<Term>,

    /// Score an candidate must reach to fire. Higher is stricter.
    pub confidence_threshold: f32,
    pub cooldown_ms: u32,
    /// Chance of actually firing once everything else passes, so repeated triggers do
    /// not feel mechanical.
    pub probability: f32,
    /// When true, a keyword match without an action word cannot fire the event.
    pub require_action_word: bool,
    pub enabled: bool,
}

impl EventDefinition {
    /// A minimal event, for tests and for the editor's "new event" button.
    pub fn new(id: &str, display_name: &str) -> Self {
        Self {
            id: id.to_string(),
            display_name: display_name.to_string(),
            category: String::new(),
            kind: EventKind::OneShot,
            phrases: Vec::new(),
            terms: Vec::new(),
            confidence_threshold: 0.82,
            cooldown_ms: 3_000,
            probability: 1.0,
            require_action_word: true,
            enabled: true,
        }
    }

    pub fn with_phrases(mut self, phrases: Vec<Phrase>) -> Self {
        self.phrases = phrases;
        self
    }

    pub fn with_terms(mut self, terms: Vec<Term>) -> Self {
        self.terms = terms;
        self
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    pub fn with_cooldown(mut self, cooldown_ms: u32) -> Self {
        self.cooldown_ms = cooldown_ms;
        self
    }

    pub fn requiring_action(mut self, require: bool) -> Self {
        self.require_action_word = require;
        self
    }

    pub fn terms_of(&self, kind: TermKind) -> impl Iterator<Item = &Term> {
        self.terms.iter().filter(move |term| term.kind == kind)
    }

    pub fn examples(&self) -> impl Iterator<Item = &Phrase> {
        self.phrases.iter().filter(|phrase| !phrase.is_command)
    }

    pub fn commands(&self) -> impl Iterator<Item = &Phrase> {
        self.phrases.iter().filter(|phrase| phrase.is_command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_tags_round_trip() {
        for lang in [Lang::En, Lang::Uk, Lang::Any] {
            assert_eq!(Lang::parse(lang.as_str()), lang);
        }
        assert_eq!(Lang::parse("EN"), Lang::En);
        assert_eq!(Lang::parse("klingon"), Lang::Any);
    }

    #[test]
    fn a_new_event_is_strict_by_default() {
        let event = EventDefinition::new("OPEN_DOOR", "Open Door");

        // A missed sound beats a wrong one.
        assert!(event.confidence_threshold >= 0.8);
        assert!(event.require_action_word);
        assert!(event.enabled);
        assert_eq!(event.probability, 1.0);
    }

    #[test]
    fn terms_and_phrases_are_filterable_by_role() {
        let event = EventDefinition::new("SWORD_SWING", "Sword Swing")
            .with_terms(vec![
                Term::keyword("sword"),
                Term::action("swings"),
                Term::negative("lying on"),
            ])
            .with_phrases(vec![
                Phrase::example(Lang::En, "swings his sword"),
                Phrase::command("sound sword"),
            ]);

        assert_eq!(event.terms_of(TermKind::Keyword).count(), 1);
        assert_eq!(event.terms_of(TermKind::Action).count(), 1);
        assert_eq!(event.terms_of(TermKind::Negative).count(), 1);
        assert_eq!(event.examples().count(), 1);
        assert_eq!(event.commands().count(), 1);
    }

    #[test]
    fn definitions_serialize_for_the_editor() {
        let event =
            EventDefinition::new("THUNDER", "Thunder").with_terms(vec![Term::keyword("thunder")]);

        let json = serde_json::to_string(&event).expect("serialize");
        let restored: EventDefinition = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, event);
    }
}
