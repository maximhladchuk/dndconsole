//! Light stemming for Ukrainian and English.
//!
//! The goal is *consistency*, not linguistic correctness: event phrases and transcripts
//! both go through this, so "відчиняє", "відчинив" and "відчиняти" only need to collapse
//! to the same string as each other. A full morphological analyser would be a large
//! dependency and a large model download for a gain we cannot measure.
//!
//! The risk of a rule-based stemmer is over-stemming — chopping so much that unrelated
//! words collide, which produces exactly the false positives this project is built to
//! avoid. Two guards against that: a minimum stem length, and suffix lists that are
//! matched longest-first.

/// Never stem below this many characters.
///
/// Three is the smallest value that still collapses the pairs we need — "runs"/"running",
/// "меча"/"меч" — while leaving genuinely short words ("the", "не", "меч") untouched.
/// Four looks safer and is not: it silently leaves four-letter inflections unstemmed.
const MIN_STEM_CHARS: usize = 3;

/// A suffix of two characters or more must leave at least this much behind.
///
/// One suffix list serves both nouns and verbs, matched longest-first, and that lets a
/// verb rule fire on a noun: `келих` lost its genitive-plural `-их` and became `кел`,
/// while `келиха` lost only `-а` and stayed `келих`. Two forms of one word, two
/// different keys, and a keyword that never matched what was said.
///
/// Requiring a longer remainder for a longer suffix blocks exactly that class of
/// over-stemming while leaving the short, safe endings alone.
///
/// Relaxing this to three was tried and reverted. Ukrainian has many three-letter roots
/// — `меч`, `щит`, `кін` — whose instrumental and genitive-plural forms it would then
/// stem correctly, but `-их` is also two characters, and `келих` went straight back to
/// `кел`. There is no length that separates the two cases, so the case forms of short
/// nouns are listed in `seed.rs` instead, where a wrong entry is visible.
const MIN_STEM_CHARS_LONG_SUFFIX: usize = 4;

/// Ukrainian suffixes, longest first. Verb endings before noun endings, because a verb
/// ending is usually longer and more distinctive.
const UKRAINIAN_SUFFIXES: &[&str] = &[
    // Sorted longest-first: the first match wins, so order is correctness, not style.
    //
    // The bare infinitive endings "ти" and "чи" are deliberately absent. They unify
    // nothing — "відчинити" and "відчиняє" reduce to different roots either way — and
    // they ate the plural of every noun ending the same: "монети" became "моне" while
    // "монета" became "монет", so a keyword never matched its own plural.
    "відкриває скриню",
    "розлетілося",
    "розчахнула",
    "відчинити",
    "піднімаєш",
    "розбилась",
    "розчахнув",
    "відчиняє",
    "піднімає",
    "розчахну",
    "підніма",
    "увалась",
    "увалися",
    "уватися",
    "ювалися",
    "юватися",
    "валася",
    "монета",
    "монети",
    "нулася",
    "нулись",
    "нулися",
    "нулось",
    "нулося",
    "піднім",
    "уються",
    "яються",
    "яємося",
    "яється",
    "-нула",
    "ались",
    "атися",
    "вався",
    "илась",
    "илася",
    "илися",
    "илось",
    "итися",
    "монет",
    "нувся",
    "ували",
    "увати",
    "ювали",
    "ювати",
    "ються",
    "ялась",
    "ялися",
    "ємось",
    "ється",
    "ілась",
    "ілася",
    "ілися",
    "ілось",
    "ілося",
    "ішими",
    "-нув",
    "ався",
    "аємо",
    "аєте",
    "ешся",
    "ився",
    "лась",
    "лись",
    "лися",
    "лося",
    "моне",
    "нула",
    "нули",
    "нуло",
    "ував",
    "уємо",
    "уєте",
    "ював",
    "явся",
    "яють",
    "яємо",
    "яєте",
    "-єш",
    "ала",
    "али",
    "ало",
    "ами",
    "ати",
    "ать",
    "аєш",
    "вся",
    "еві",
    "ела",
    "ели",
    "ело",
    "ему",
    "ила",
    "или",
    "ило",
    "има",
    "ими",
    "имо",
    "ити",
    "ить",
    "нув",
    "ові",
    "ого",
    "ому",
    "ула",
    "ули",
    "уло",
    "ути",
    "уєш",
    "ьми",
    "юти",
    "ють",
    "ями",
    "яти",
    "ять",
    "яєш",
    "ємо",
    "іла",
    "іли",
    "іло",
    "іми",
    "іти",
    "ією",
    "-в",
    "ав",
    "ам",
    "ах",
    "ає",
    "ей",
    "ем",
    "еш",
    "ею",
    "ив",
    "ий",
    "их",
    "иш",
    "ка",
    "ки",
    "ко",
    "ку",
    "ла",
    "ли",
    "ло",
    "ов",
    "ом",
    "ою",
    "ої",
    "ює",
    "яв",
    "ям",
    "ях",
    "яє",
    "єш",
    "ів",
    "ій",
    "іх",
    "їш",
    "а",
    "е",
    "и",
    "о",
    "у",
    "ь",
    "ю",
    "я",
    "є",
    "і",
    "ї",
];

/// English suffixes, longest first.
const ENGLISH_SUFFIXES: &[&str] = &[
    "ingly", "edly", "ings", "ings", "ing", "ies", "ied", "ers", "est", "ed", "es", "er", "s",
];

/// Reduce a token to a comparison key.
pub fn stem(token: &str) -> String {
    if token.chars().any(is_cyrillic) {
        stem_with(token, UKRAINIAN_SUFFIXES, false)
    } else {
        stem_with(token, ENGLISH_SUFFIXES, true)
    }
}

fn is_cyrillic(character: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&character)
}

fn stem_with(token: &str, suffixes: &[&str], undouble: bool) -> String {
    let lowered = token.to_lowercase();
    let length = lowered.chars().count();

    if length <= MIN_STEM_CHARS {
        return lowered;
    }

    for suffix in suffixes {
        let suffix_length = suffix.chars().count();
        let floor = if suffix_length >= 2 {
            MIN_STEM_CHARS_LONG_SUFFIX
        } else {
            MIN_STEM_CHARS
        };
        if length.saturating_sub(suffix_length) < floor {
            continue;
        }
        if let Some(stripped) = lowered.strip_suffix(suffix) {
            let stripped = stripped.to_string();
            return if undouble {
                undouble_final(&stripped)
            } else {
                stripped
            };
        }
    }

    lowered
}

/// "runn" from "running" should be "run".
fn undouble_final(stem: &str) -> String {
    let characters: Vec<char> = stem.chars().collect();
    if characters.len() > MIN_STEM_CHARS {
        let last = characters[characters.len() - 1];
        let previous = characters[characters.len() - 2];
        if last == previous && !"aeiou".contains(last) {
            return characters[..characters.len() - 1].iter().collect();
        }
    }
    stem.to_string()
}

/// Stem a whole phrase, for comparing phrase stems against transcript stems.
pub fn stem_phrase(phrase: &str) -> Vec<String> {
    phrase.split_whitespace().map(stem).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_same(words: &[&str]) -> bool {
        let stems: Vec<String> = words.iter().map(|w| stem(w)).collect();
        let same = stems.windows(2).all(|w| w[0] == w[1]);
        if !same {
            eprintln!("{words:?} -> {stems:?}");
        }
        same
    }

    #[test]
    fn ukrainian_verb_forms_of_opening_collapse_together() {
        assert!(all_same(&[
            "відчиняти",
            "відчиняє",
            "відчиняєте",
            "відчиняють",
            "відчинив",
            "відчинили",
        ]));
    }

    #[test]
    fn ukrainian_noun_cases_of_door_collapse_together() {
        assert!(all_same(&[
            "двері",
            "дверей",
            "дверима",
            "дверях",
            "дверьми"
        ]));
    }

    #[test]
    fn ukrainian_short_noun_cases_collapse_too() {
        assert!(all_same(&["меч", "меча", "мечу", "мечі"]));
    }

    #[test]
    fn english_verb_forms_collapse_together() {
        assert!(all_same(&["opens", "opened", "opening"]));
        assert!(all_same(&["swing", "swings", "swinging"]));
        assert!(all_same(&["attack", "attacks", "attacked", "attacking"]));
    }

    #[test]
    fn english_plurals_collapse_to_the_singular() {
        assert_eq!(stem("doors"), stem("door"));
        assert_eq!(stem("swords"), stem("sword"));
        assert_eq!(stem("arrows"), stem("arrow"));
    }

    #[test]
    fn irregular_plurals_are_a_known_limitation() {
        // "wolves" and "wolf" do not collapse: a rule-based stemmer cannot do that
        // without an irregular-forms table. Events cover it by listing both words as
        // keywords instead, which is cheaper and more honest than half a lemmatizer.
        assert_ne!(stem("wolves"), stem("wolf"));
        assert_eq!(stem("wolves"), stem("wolves"));
    }

    #[test]
    fn doubled_consonants_are_undoubled() {
        assert_eq!(stem("running"), stem("runs"));
    }

    #[test]
    fn short_words_are_left_alone() {
        // Over-stemming short words is how unrelated words start colliding.
        for word in ["the", "is", "a", "він", "не", "меч", "дуб"] {
            assert_eq!(
                stem(word),
                word.to_lowercase(),
                "{word} should not be stemmed"
            );
        }
    }

    #[test]
    fn unrelated_words_do_not_collide() {
        // The failure mode that matters: two different words stemming to one key would
        // make an event fire on the wrong noun.
        let pairs = [
            ("двері", "дерево"),
            ("sword", "swore"),
            ("door", "doom"),
            ("вовк", "вода"),
            ("thunder", "thumb"),
        ];
        for (a, b) in pairs {
            assert_ne!(stem(a), stem(b), "{a} and {b} must not stem alike");
        }
    }

    #[test]
    fn stemming_is_idempotent() {
        for word in ["відчиняє", "opening", "swords", "дверей", "attacked"] {
            let once = stem(word);
            assert_eq!(stem(&once), once, "{word} stemmed twice should not change");
        }
    }

    #[test]
    fn phrases_stem_word_by_word() {
        assert_eq!(
            stem_phrase("opens the doors"),
            vec![stem("opens"), "the".to_string(), stem("doors")]
        );
        assert!(stem_phrase("").is_empty());
    }

    #[test]
    fn mixed_language_text_stems_each_word_by_its_script() {
        let stems = stem_phrase("гоблін дістає sword");
        assert_eq!(stems.len(), 3);
        assert_eq!(stems[2], stem("sword"));
    }
}
