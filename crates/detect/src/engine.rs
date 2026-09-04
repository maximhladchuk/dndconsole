//! The layered detector.
//!
//! Narration comes in, scored candidates come out. Every candidate carries the reason
//! for its score — accepted or rejected — because tuning detection by guesswork is
//! hopeless, and Debug Mode renders exactly this.
//!
//! The layers, strongest first:
//!
//! | Layer | What it catches | Score |
//! |---|---|---|
//! | Command | a deliberate spoken command | 1.00 |
//! | Exact phrase | the narration contains an example phrase verbatim | 0.95 |
//! | Stem phrase | the same phrase in a different inflection | 0.88 |
//! | Fuzzy | the same phrase with a transcription error | 0.60–0.85 |
//! | Keyword | the object is mentioned; the action decides | 0.55 + 0.30 |
//!
//! Scores do not stack. The strongest layer wins and gates apply on top, so two weak
//! signals stay weak — an important property when the cost of a wrong sound is high.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use serde::Serialize;

use crate::event::{EventDefinition, TermKind};
use crate::fuzzy::best_window_similarity;
use crate::normalize::{normalize, normalize_text, Normalized};
use crate::stem::stem_phrase;

/// Something that can judge how close a transcript is to an event's example phrases,
/// without sharing any words with them.
///
/// Implemented by `dndsound-semantic` with a local embedding model. Declared here, and
/// used through a trait object, so this crate stays pure: the corpus tests run without
/// ONNX Runtime, without a 113 MB model, and in milliseconds.
pub trait SemanticScorer: Send + Sync {
    /// Best similarity per event, strongest first. Scores are cosine similarities in
    /// roughly `0.0..=1.0`.
    fn similar_events(&self, transcript: &str) -> Vec<(String, f32)>;
}

/// Words that frame narration as memory, hypothesis or question rather than something
/// happening now. "You remember the sound of wolves" should not summon wolves.
const IRREALIS_MARKERS: &[&str] = &[
    // English
    "remember",
    "remembered",
    "imagine",
    "imagined",
    "if",
    "would",
    "could",
    "might",
    "used to",
    "dream",
    "dreamed",
    "story",
    "legend",
    "rumour",
    "rumor",
    "heard about",
    "picture of",
    "painting of",
    "drawing of",
    "statue of",
    // Ukrainian
    "пам'ята",
    "пригада",
    "уяв",
    "якби",
    "якщо",
    "мабуть",
    "легенда",
    "історія",
    "розповіда",
    "малюнок",
    "картина",
    "статуя",
    "згада",
];

const SCORE_COMMAND: f32 = 1.0;
const SCORE_EXACT_PHRASE: f32 = 0.95;
const SCORE_STEM_PHRASE: f32 = 0.88;
const SCORE_KEYWORD: f32 = 0.55;
/// What an action word adds to a bare keyword match.
const ACTION_BONUS: f32 = 0.30;
/// Fuzzy matches below this similarity are not considered at all.
const FUZZY_FLOOR: f32 = 0.80;
/// Penalty applied when the utterance is framed as memory or hypothesis.
const IRREALIS_PENALTY: f32 = 0.40;

/// Cosine similarity below which the semantic layer says nothing.
///
/// Calibrated against real embeddings — see `crates/semantic/tests/calibration.rs`,
/// which prints the similarity of matching and non-matching narration and fails if the
/// two ranges start to overlap.
const SEMANTIC_FLOOR: f32 = 0.86;
/// Semantic similarity is mapped into this confidence range, so it can support a
/// keyword match but never outrank a phrase the user actually wrote down.
///
/// The ceiling sits deliberately *below* `SCORE_KEYWORD + ACTION_BONUS` (0.85), which is
/// what a literal keyword with a verb next to it scores. Semantic similarity is the
/// weakest evidence in the system — it exists to reach phrasings nobody wrote down — and
/// it must never beat a word the narration actually contained.
///
/// This was not theoretical. With a ceiling of 0.87, "Вона відкриває двері до зали"
/// matched OPEN_DOOR on the literal word "двері" at 0.85 and lost to CHEST_OPEN at 0.87,
/// because opening a door and opening a chest are all but identical to an embedding
/// model. Two events swapped places in the corpus, in both directions.
const SEMANTIC_MIN_SCORE: f32 = 0.60;
const SEMANTIC_MAX_SCORE: f32 = 0.84;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchLayer {
    Command,
    ExactPhrase,
    StemPhrase,
    Fuzzy,
    Semantic,
    Keyword,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "reason",
    content = "detail"
)]
pub enum RejectionReason {
    BelowThreshold { score: f32, threshold: f32 },
    NegativePhrase(String),
    NoActionWord,
    FramedAsMemoryOrHypothesis(String),
    Disabled,
}

impl RejectionReason {
    /// A sentence for the Debug Mode panel.
    pub fn explain(&self) -> String {
        match self {
            RejectionReason::BelowThreshold { score, threshold } => {
                format!("confidence {score:.2} is below the threshold of {threshold:.2}")
            }
            RejectionReason::NegativePhrase(phrase) => {
                format!("the narration contains the negative phrase '{phrase}'")
            }
            RejectionReason::NoActionWord => {
                "the object was mentioned but nothing was done with it".to_string()
            }
            RejectionReason::FramedAsMemoryOrHypothesis(marker) => {
                format!("'{marker}' frames this as memory or hypothesis, not action")
            }
            RejectionReason::Disabled => "the event is disabled".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub event_id: String,
    pub confidence: f32,
    pub threshold: f32,
    pub layer: MatchLayer,
    /// The text that produced the match, used for duplicate suppression.
    pub matched_span: String,
    pub accepted: bool,
    pub rejection: Option<RejectionReason>,
    /// Whether an action word was found, shown in Debug Mode.
    pub action_word: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectionInput<'a> {
    pub transcript: &'a str,
    /// False while the speaker is still talking, true for the settled transcript.
    pub is_final: bool,
    pub timestamp_ms: i64,
}

impl<'a> DetectionInput<'a> {
    pub fn final_transcript(transcript: &'a str, timestamp_ms: i64) -> Self {
        Self {
            transcript,
            is_final: true,
            timestamp_ms,
        }
    }

    pub fn partial(transcript: &'a str, timestamp_ms: i64) -> Self {
        Self {
            transcript,
            is_final: false,
            timestamp_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub transcript: String,
    pub normalized: String,
    pub is_final: bool,
    pub timestamp_ms: i64,
    /// Every candidate considered, accepted first, then by descending confidence.
    pub candidates: Vec<Candidate>,
    /// How long detection took, in microseconds. Measured, never estimated.
    pub elapsed_us: u64,
}

impl Detection {
    pub fn accepted(&self) -> impl Iterator<Item = &Candidate> {
        self.candidates
            .iter()
            .filter(|candidate| candidate.accepted)
    }

    pub fn best(&self) -> Option<&Candidate> {
        self.accepted().next()
    }
}

/// Precomputed lookup from a word stem to the events that care about it.
///
/// This is what keeps detection cheap with thousands of events: a transcript touches a
/// handful of stems, so only a handful of events are ever scored.
#[derive(Debug, Default)]
struct InvertedIndex {
    by_stem: HashMap<String, HashSet<usize>>,
}

impl InvertedIndex {
    fn build(events: &[EventDefinition]) -> Self {
        let mut by_stem: HashMap<String, HashSet<usize>> = HashMap::new();

        for (position, event) in events.iter().enumerate() {
            let mut add = |text: &str| {
                for stem in stem_phrase(&normalize_text(text)) {
                    by_stem.entry(stem).or_default().insert(position);
                }
            };

            for term in &event.terms {
                if term.kind != TermKind::Negative {
                    add(&term.text);
                }
            }
            for phrase in &event.phrases {
                add(&phrase.text);
            }
        }

        Self { by_stem }
    }

    fn candidates(&self, normalized: &Normalized) -> HashSet<usize> {
        let mut found = HashSet::new();
        for stem in &normalized.stems {
            if let Some(events) = self.by_stem.get(stem) {
                found.extend(events.iter().copied());
            }
        }
        found
    }
}

/// Prepared form of an event, so phrases are not re-normalized per transcript.
struct Prepared {
    definition: EventDefinition,
    exact_phrases: Vec<String>,
    command_phrases: Vec<String>,
    phrase_stems: Vec<Vec<String>>,
    keywords: Vec<Vec<String>>,
    actions: Vec<Vec<String>>,
    negatives: Vec<String>,
}

pub struct Detector {
    events: Vec<Prepared>,
    index: InvertedIndex,
    /// 0.0 strictest, 1.0 loosest, 0.5 neutral.
    sensitivity: f32,
    /// Optional layer 4. Absent until the embedding model is downloaded, and the
    /// detector works without it — just with less reach.
    semantic: Option<std::sync::Arc<dyn SemanticScorer>>,
}

impl Detector {
    pub fn new(events: Vec<EventDefinition>) -> Self {
        let index = InvertedIndex::build(&events);
        let events = events.into_iter().map(prepare).collect();
        Self {
            events,
            index,
            sensitivity: 0.5,
            semantic: None,
        }
    }

    /// Attach the semantic layer.
    pub fn set_semantic(&mut self, semantic: Option<std::sync::Arc<dyn SemanticScorer>>) {
        self.semantic = semantic;
    }

    pub fn has_semantic(&self) -> bool {
        self.semantic.is_some()
    }

    /// Global bias on every event's threshold. Above 0.5 triggers more readily.
    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.sensitivity = sensitivity.clamp(0.0, 1.0);
    }

    pub fn sensitivity(&self) -> f32 {
        self.sensitivity
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn definitions(&self) -> impl Iterator<Item = &EventDefinition> {
        self.events.iter().map(|prepared| &prepared.definition)
    }

    /// Score a transcript against every plausible event.
    pub fn detect(&self, input: DetectionInput<'_>) -> Detection {
        let started = std::time::Instant::now();
        let normalized = normalize(input.transcript);

        let mut candidates = Vec::new();

        if !normalized.is_empty() {
            let irrealis = irrealis_marker(&normalized);

            // One embedding for the whole transcript, reused across every event.
            let semantic: HashMap<String, f32> = match &self.semantic {
                Some(scorer) => scorer
                    .similar_events(input.transcript)
                    .into_iter()
                    .filter(|(_, score)| *score >= SEMANTIC_FLOOR)
                    .collect(),
                None => HashMap::new(),
            };

            // Cheap retrieval first, then anything the semantic layer surfaced that
            // shares no words with the transcript at all.
            let mut positions = self.index.candidates(&normalized);
            if !semantic.is_empty() {
                for (position, prepared) in self.events.iter().enumerate() {
                    if semantic.contains_key(&prepared.definition.id) {
                        positions.insert(position);
                    }
                }
            }

            for position in positions {
                let prepared = &self.events[position];
                let similarity = semantic.get(&prepared.definition.id).copied();
                if let Some(candidate) =
                    self.score(prepared, &normalized, irrealis.as_deref(), similarity)
                {
                    candidates.push(candidate);
                }
            }
        }

        // Accepted candidates first, then by confidence. The UI reads this order
        // directly, and so does the trigger engine.
        candidates.sort_by(|a, b| {
            b.accepted
                .cmp(&a.accepted)
                .then(b.confidence.total_cmp(&a.confidence))
                .then(a.event_id.cmp(&b.event_id))
        });

        Detection {
            transcript: input.transcript.to_string(),
            normalized: normalized.text,
            is_final: input.is_final,
            timestamp_ms: input.timestamp_ms,
            candidates,
            elapsed_us: started.elapsed().as_micros() as u64,
        }
    }

    fn score(
        &self,
        prepared: &Prepared,
        normalized: &Normalized,
        irrealis: Option<&str>,
        semantic_similarity: Option<f32>,
    ) -> Option<Candidate> {
        let event = &prepared.definition;
        let threshold = self.effective_threshold(event.confidence_threshold);

        if !event.enabled {
            return Some(Candidate {
                event_id: event.id.clone(),
                confidence: 0.0,
                threshold,
                layer: MatchLayer::Keyword,
                matched_span: String::new(),
                accepted: false,
                rejection: Some(RejectionReason::Disabled),
                action_word: None,
            });
        }

        // A deliberate command wins outright, before anything else is considered.
        if let Some(command) = prepared
            .command_phrases
            .iter()
            .find(|phrase| normalized.contains_phrase(phrase))
        {
            return Some(Candidate {
                event_id: event.id.clone(),
                confidence: SCORE_COMMAND,
                threshold,
                layer: MatchLayer::Command,
                matched_span: command.clone(),
                accepted: true,
                rejection: None,
                action_word: None,
            });
        }

        // A negative phrase vetoes the event no matter how strong the evidence.
        if let Some(negative) = prepared
            .negatives
            .iter()
            .find(|phrase| normalized.contains_phrase(phrase))
        {
            return Some(Candidate {
                event_id: event.id.clone(),
                confidence: 0.0,
                threshold,
                layer: MatchLayer::Keyword,
                matched_span: negative.clone(),
                accepted: false,
                rejection: Some(RejectionReason::NegativePhrase(negative.clone())),
                action_word: None,
            });
        }

        // Whether any action word is present at all. This only ranks the fuzzy layer
        // against the keyword one below; the action that actually opens the gate is
        // chosen afterwards, once the keyword is known, because it must be a different
        // word from the keyword.
        let any_action = prepared
            .actions
            .iter()
            .any(|action| normalized.contains_stems(action));

        let (mut score, layer, span, keyword_span) =
            self.best_layer(prepared, normalized, semantic_similarity, any_action)?;

        // "Обладунки дзвонять" was ringing a church bell: BELL lists "дзвони" as an
        // object and "дзвонить" as an action, both of which stem to дзвон, so the one
        // spoken verb answered both halves of the gate. An action has to be done to the
        // object by some *other* word.
        let action_word = prepared
            .actions
            .iter()
            .find(|action| {
                normalized
                    .find_stems_outside(action, keyword_span.as_ref())
                    .is_some()
            })
            .or_else(|| {
                // Unless the event declares that one word is both. SCREAM's objects are
                // its own verbs — "заверещала" is the thing and the doing of it, and
                // there is no second word to point at. The event says so by listing the
                // same term as a keyword and as an action, which is a decision someone
                // made, not a stemmer accident: BELL's "дзвони" and "дзвонить" are two
                // different entries that happen to collapse together, and stay barred.
                let stems = &normalized.stems[keyword_span.clone()?];
                prepared
                    .actions
                    .iter()
                    .find(|action| action.as_slice() == stems)
            })
            .map(|action| action.join(" "));

        // A bare keyword match and a semantic match both need the action gate. A phrase
        // match does not: the phrase itself already describes the action. Semantic
        // similarity, by contrast, happily rates "a sword lies on the table" as close to
        // "he swings his sword" — they are about the same things.
        let mut rejection = None;
        if matches!(layer, MatchLayer::Keyword | MatchLayer::Semantic) {
            match &action_word {
                Some(_) => {
                    score += ACTION_BONUS;
                    // The gate *requires* an action word for a semantic match; it does
                    // not *reward* one. Letting the bonus through put a semantic match at
                    // 1.14 and back above every literal one, which is exactly the ranking
                    // the ceiling exists to prevent — an embedding cannot tell "opens the
                    // door" from "opens the chest", and the literal word "двері" can.
                    if layer == MatchLayer::Semantic {
                        score = score.min(SEMANTIC_MAX_SCORE);
                    }
                }
                None if event.require_action_word => {
                    rejection = Some(RejectionReason::NoActionWord);
                }
                None => {}
            }
        }

        if let Some(marker) = irrealis {
            score -= IRREALIS_PENALTY;
            if score < threshold && rejection.is_none() {
                rejection = Some(RejectionReason::FramedAsMemoryOrHypothesis(
                    marker.to_string(),
                ));
            }
        }

        let score = score.clamp(0.0, 1.0);

        if rejection.is_none() && score < threshold {
            rejection = Some(RejectionReason::BelowThreshold { score, threshold });
        }

        Some(Candidate {
            event_id: event.id.clone(),
            confidence: score,
            threshold,
            layer,
            matched_span: span,
            accepted: rejection.is_none(),
            rejection,
            action_word,
        })
    }

    /// The strongest layer that matches, if any.
    /// Pick the layer that will end up scoring highest.
    ///
    /// `has_action` matters because the caller adds `ACTION_BONUS` to keyword matches and
    /// not to phrase ones. Returning the first layer that matched, as this used to,
    /// picked a fuzzy match worth 0.70 over a keyword match worth 0.55 + 0.30 = 0.85 —
    /// "небо розколола блискавиця" was a near-miss on a written phrase and an exact hit
    /// on a keyword with a verb, and the near-miss won and fell below the threshold.
    ///
    /// The fourth element is where the keyword sat, for the layers that matched on one.
    /// The caller needs it to keep an action word from being the keyword itself.
    fn best_layer(
        &self,
        prepared: &Prepared,
        normalized: &Normalized,
        semantic_similarity: Option<f32>,
        has_action: bool,
    ) -> Option<(f32, MatchLayer, String, Option<Range<usize>>)> {
        if let Some(phrase) = prepared
            .exact_phrases
            .iter()
            .find(|phrase| normalized.contains_phrase(phrase))
        {
            return Some((
                SCORE_EXACT_PHRASE,
                MatchLayer::ExactPhrase,
                phrase.clone(),
                None,
            ));
        }

        for (stems, original) in prepared.phrase_stems.iter().zip(&prepared.exact_phrases) {
            if normalized.contains_stems(stems) {
                return Some((
                    SCORE_STEM_PHRASE,
                    MatchLayer::StemPhrase,
                    original.clone(),
                    None,
                ));
            }
        }

        let mut best_fuzzy = 0.0;
        let mut best_phrase = String::new();
        for (stems, original) in prepared.phrase_stems.iter().zip(&prepared.exact_phrases) {
            let similarity = best_window_similarity(&normalized.stems, stems);
            if similarity > best_fuzzy {
                best_fuzzy = similarity;
                best_phrase = original.clone();
            }
        }
        let keyword = prepared
            .keywords
            .iter()
            .find_map(|keyword| normalized.find_stems(keyword).map(|span| (keyword, span)));

        if best_fuzzy >= FUZZY_FLOOR {
            // Map 0.80..1.00 similarity onto 0.60..0.85 confidence, so a fuzzy match is
            // always weaker than the stem match it approximates.
            let confidence = 0.60 + (best_fuzzy - FUZZY_FLOOR) / (1.0 - FUZZY_FLOOR) * 0.25;

            let keyword_total = SCORE_KEYWORD + if has_action { ACTION_BONUS } else { 0.0 };
            if keyword.is_none() || confidence >= keyword_total {
                return Some((confidence, MatchLayer::Fuzzy, best_phrase, None));
            }
        }

        if let Some((keyword, span)) = keyword {
            return Some((
                SCORE_KEYWORD,
                MatchLayer::Keyword,
                keyword.join(" "),
                Some(span),
            ));
        }

        // Nothing matched literally. The semantic layer is the last chance, and the only
        // one that can reach a phrasing nobody wrote down.
        let similarity = semantic_similarity?;
        let range = SEMANTIC_MAX_SCORE - SEMANTIC_MIN_SCORE;
        let confidence =
            SEMANTIC_MIN_SCORE + ((similarity - SEMANTIC_FLOOR) / (1.0 - SEMANTIC_FLOOR)) * range;

        Some((
            confidence.clamp(SEMANTIC_MIN_SCORE, SEMANTIC_MAX_SCORE),
            MatchLayer::Semantic,
            format!("semantic {similarity:.2}"),
            None,
        ))
    }

    /// Apply the global sensitivity bias to an event's own threshold.
    fn effective_threshold(&self, threshold: f32) -> f32 {
        (threshold + (0.5 - self.sensitivity) * 0.3).clamp(0.30, 0.99)
    }
}

fn prepare(definition: EventDefinition) -> Prepared {
    let exact_phrases: Vec<String> = definition
        .examples()
        .map(|phrase| normalize_text(&phrase.text))
        .filter(|text| !text.is_empty())
        .collect();

    let command_phrases: Vec<String> = definition
        .commands()
        .map(|phrase| normalize_text(&phrase.text))
        .filter(|text| !text.is_empty())
        .collect();

    let phrase_stems: Vec<Vec<String>> = exact_phrases
        .iter()
        .map(|phrase| stem_phrase(phrase))
        .collect();

    let collect = |kind: TermKind| -> Vec<Vec<String>> {
        definition
            .terms_of(kind)
            .map(|term| stem_phrase(&normalize_text(&term.text)))
            .filter(|stems| !stems.is_empty())
            .collect()
    };

    let negatives: Vec<String> = definition
        .terms_of(TermKind::Negative)
        .map(|term| normalize_text(&term.text))
        .filter(|text| !text.is_empty())
        .collect();

    Prepared {
        keywords: collect(TermKind::Keyword),
        actions: collect(TermKind::Action),
        negatives,
        exact_phrases,
        command_phrases,
        phrase_stems,
        definition,
    }
}

/// Find the first marker that frames the utterance as something other than present action.
fn irrealis_marker(normalized: &Normalized) -> Option<String> {
    IRREALIS_MARKERS
        .iter()
        .find(|marker| {
            if marker.contains(' ') {
                normalized.contains_phrase(marker)
            } else {
                // Prefix match so Ukrainian stems like "пам'ята" catch their inflections.
                normalized
                    .tokens
                    .iter()
                    .any(|token| token == *marker || token.starts_with(*marker))
            }
        })
        .map(|marker| marker.to_string())
}
