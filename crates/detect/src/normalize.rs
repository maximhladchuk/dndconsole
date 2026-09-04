//! Turning a raw transcript into something matchable.
//!
//! Everything downstream compares normalized text against normalized phrases, so this
//! module defines what "the same words" means. It is deliberately conservative: it
//! removes noise (case, punctuation, apostrophe variants, decomposed characters) without
//! removing meaning.

use unicode_normalization::UnicodeNormalization;

/// A transcript, prepared for matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    /// Lowercased, punctuation-free, single-spaced text.
    pub text: String,
    /// Word tokens of `text`.
    pub tokens: Vec<String>,
    /// Stems of `tokens`, index for index.
    pub stems: Vec<String>,
}

impl Normalized {
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Does the normalized text contain this normalized phrase, on word boundaries?
    ///
    /// Substring matching alone would let "or" match inside "sword", which is exactly
    /// the class of false positive this project exists to avoid.
    pub fn contains_phrase(&self, phrase: &str) -> bool {
        contains_word_sequence(&self.text, phrase)
    }

    /// Does the stem sequence contain this sequence of stems?
    pub fn contains_stems(&self, stems: &[String]) -> bool {
        self.find_stems(stems).is_some()
    }

    /// Where this sequence of stems sits in the token stream, if it is there at all.
    ///
    /// The range matters because a term list can say which words name the object and
    /// which name the action, but not that they have to be different words. Ukrainian
    /// makes that gap expensive: "дзвони" (bells) and "дзвонять" (they ring) reduce to
    /// the same stem, so a single spoken word satisfied both halves of the action gate
    /// and "обладунки дзвонять" rang a church bell.
    pub fn find_stems(&self, stems: &[String]) -> Option<std::ops::Range<usize>> {
        self.find_stems_outside(stems, None)
    }

    /// `find_stems`, ignoring occurrences that overlap `exclude`.
    ///
    /// Every occurrence is considered, not just the first: "дзвони дзвонять" has the
    /// same stem twice, and the second one is a perfectly good action word even though
    /// the first is the object.
    pub fn find_stems_outside(
        &self,
        stems: &[String],
        exclude: Option<&std::ops::Range<usize>>,
    ) -> Option<std::ops::Range<usize>> {
        if stems.is_empty() || stems.len() > self.stems.len() {
            return None;
        }
        self.stems
            .windows(stems.len())
            .enumerate()
            .filter(|(_, window)| *window == stems)
            .map(|(start, _)| start..start + stems.len())
            .find(|span| match exclude {
                Some(exclude) => span.end <= exclude.start || span.start >= exclude.end,
                None => true,
            })
    }

    /// Position of a stem in the token stream, if present.
    pub fn stem_position(&self, stem: &str) -> Option<usize> {
        self.stems.iter().position(|s| s == stem)
    }
}

/// Normalize a transcript.
pub fn normalize(text: &str) -> Normalized {
    let text = normalize_text(text);
    let tokens: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    let stems = tokens.iter().map(|t| crate::stem::stem(t)).collect();

    Normalized {
        text,
        tokens,
        stems,
    }
}

/// The text half of normalization, exposed so phrases can be normalized the same way.
pub fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_space = true;

    // NFC first: whisper and macOS can both emit decomposed characters, so "й" may
    // arrive as "и" plus a combining breve and would otherwise never match.
    for character in text.nfc() {
        let character = unify_apostrophe(character);

        if character.is_alphanumeric() || character == '\'' {
            for lowered in character.to_lowercase() {
                out.push(lowered);
            }
            last_was_space = false;
        } else if !last_was_space {
            out.push(' ');
            last_was_space = true;
        }
    }

    out.trim_end().to_string()
}

/// Ukrainian text arrives with any of five apostrophe-ish characters depending on the
/// keyboard, the font and the transcription. They all mean the same thing.
fn unify_apostrophe(character: char) -> char {
    match character {
        '\u{2019}' | '\u{02BC}' | '\u{02BB}' | '\u{0060}' | '\u{00B4}' | '\u{2018}' => '\'',
        other => other,
    }
}

/// Word-boundary-aware substring search over already-normalized text.
fn contains_word_sequence(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }

    let haystack_words: Vec<&str> = haystack.split(' ').filter(|w| !w.is_empty()).collect();
    let needle_words: Vec<&str> = needle.split(' ').filter(|w| !w.is_empty()).collect();

    if needle_words.is_empty() || needle_words.len() > haystack_words.len() {
        return false;
    }

    haystack_words
        .windows(needle_words.len())
        .any(|window| window == needle_words.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_and_punctuation_are_removed() {
        let normalized = normalize("You SLOWLY push open the old, wooden door!");
        assert_eq!(normalized.text, "you slowly push open the old wooden door");
        assert_eq!(normalized.tokens.len(), 8);
    }

    #[test]
    fn ukrainian_text_keeps_its_letters() {
        let normalized = normalize("Ви повільно відчиняєте старі двері.");
        assert_eq!(normalized.text, "ви повільно відчиняєте старі двері");
    }

    #[test]
    fn every_apostrophe_variant_becomes_the_same_character() {
        let variants = [
            "дерев'яні",
            "дерев\u{2019}яні",
            "дерев\u{02BC}яні",
            "дерев\u{00B4}яні",
        ];
        let normalized: Vec<String> = variants.iter().map(|v| normalize_text(v)).collect();

        assert!(
            normalized.windows(2).all(|w| w[0] == w[1]),
            "apostrophe variants should normalize identically: {normalized:?}"
        );
        assert_eq!(normalized[0], "дерев'яні");
    }

    #[test]
    fn decomposed_characters_are_composed_first() {
        // "й" as и + combining breve, which is what some transcripts contain.
        let decomposed = "и\u{0306}ти";
        let composed = "йти";
        assert_eq!(normalize_text(decomposed), normalize_text(composed));
    }

    #[test]
    fn empty_and_punctuation_only_input_normalizes_to_nothing() {
        assert!(normalize("").is_empty());
        assert!(normalize("!!! ... ???").is_empty());
        assert_eq!(normalize("   ").text, "");
    }

    #[test]
    fn digits_are_kept() {
        assert_eq!(normalize_text("You take 3 damage"), "you take 3 damage");
    }

    #[test]
    fn phrase_matching_respects_word_boundaries() {
        let normalized = normalize("He draws his sword.");

        assert!(normalized.contains_phrase("draws his sword"));
        assert!(normalized.contains_phrase("sword"));

        // The whole point: "or" is inside "sword" but is not the word "or".
        assert!(!normalized.contains_phrase("or"));
        assert!(!normalized.contains_phrase("word"));
        assert!(
            !normalized.contains_phrase("draws sword"),
            "must be contiguous"
        );
    }

    #[test]
    fn phrase_matching_handles_edges() {
        let normalized = normalize("open the door");
        assert!(normalized.contains_phrase("open"));
        assert!(normalized.contains_phrase("door"));
        assert!(normalized.contains_phrase("open the door"));
        assert!(!normalized.contains_phrase(""));
        assert!(!normalized.contains_phrase("open the door slowly"));
    }

    #[test]
    fn stem_sequences_match_across_inflection() {
        let normalized = normalize("Він відчинив двері");
        let stems: Vec<String> = ["відчиняти", "двері"]
            .iter()
            .map(|w| crate::stem::stem(w))
            .collect();

        assert!(
            normalized.contains_stems(&stems),
            "stems {:?} should appear in {:?}",
            stems,
            normalized.stems
        );
    }
}
