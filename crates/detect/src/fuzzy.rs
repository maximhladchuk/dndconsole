//! Fuzzy matching, to absorb transcription errors.
//!
//! Speech recognition mishears. "двері" comes back as "дверi" with a Latin i, "sword"
//! as "swore". Layers 1 and 2 miss those; this catches them without opening the door to
//! matching anything vaguely similar.

/// Levenshtein distance over characters, with an early exit once `limit` is exceeded.
///
/// The early exit matters: this runs over every phrase of every candidate event, and
/// most comparisons are hopeless within a few characters.
pub fn distance_within(a: &str, b: &str, limit: usize) -> Option<usize> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    if a.len().abs_diff(b.len()) > limit {
        return None;
    }
    if a.is_empty() {
        return (b.len() <= limit).then_some(b.len());
    }
    if b.is_empty() {
        return (a.len() <= limit).then_some(a.len());
    }

    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];

    for (i, &ca) in a.iter().enumerate() {
        current[0] = i + 1;
        let mut row_best = current[0];

        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (current[j] + 1)
                .min(previous[j + 1] + 1)
                .min(previous[j] + cost);
            row_best = row_best.min(current[j + 1]);
        }

        if row_best > limit {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }

    let distance = previous[b.len()];
    (distance <= limit).then_some(distance)
}

/// Similarity in `0.0..=1.0`, where 1.0 is identical.
pub fn similarity(a: &str, b: &str) -> f32 {
    let longest = a.chars().count().max(b.chars().count());
    if longest == 0 {
        return 1.0;
    }

    // Budget scales with length so short words cannot fuzz into each other: "or" and
    // "on" are one edit apart but mean entirely different things.
    let budget = budget_for(longest);
    match distance_within(a, b, budget) {
        Some(distance) => 1.0 - (distance as f32 / longest as f32),
        None => 0.0,
    }
}

/// How many edits are tolerable for a word of this length.
fn budget_for(length: usize) -> usize {
    match length {
        0..=3 => 0,
        4..=6 => 1,
        7..=10 => 2,
        _ => 3,
    }
}

/// Best similarity between `needle` and any window of `haystack` the same length.
///
/// Both are token slices, so this compares phrases rather than raw characters.
pub fn best_window_similarity(haystack: &[String], needle: &[String]) -> f32 {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0.0;
    }

    haystack
        .windows(needle.len())
        .map(|window| {
            let total: f32 = window
                .iter()
                .zip(needle)
                .map(|(a, b)| similarity(a, b))
                .sum();
            total / needle.len() as f32
        })
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn identical_strings_are_zero_distance_and_full_similarity() {
        assert_eq!(distance_within("sword", "sword", 2), Some(0));
        assert_eq!(similarity("sword", "sword"), 1.0);
        assert_eq!(similarity("", ""), 1.0);
    }

    #[test]
    fn one_typo_is_within_budget_for_a_medium_word() {
        assert_eq!(distance_within("sword", "swore", 2), Some(1));
        assert!(similarity("sword", "swore") > 0.75);
    }

    #[test]
    fn short_words_get_no_budget_at_all() {
        // "or" vs "on" is one edit, and letting that match would be a false positive
        // generator.
        assert_eq!(similarity("or", "on"), 0.0);
        assert_eq!(similarity("cat", "cut"), 0.0);
        assert_eq!(similarity("не", "на"), 0.0);
    }

    #[test]
    fn wildly_different_strings_score_zero_and_exit_early() {
        assert_eq!(distance_within("door", "thunderstorm", 2), None);
        assert_eq!(similarity("door", "thunderstorm"), 0.0);
    }

    #[test]
    fn a_latin_letter_smuggled_into_cyrillic_still_matches() {
        // A real transcription artefact: Latin "i" inside a Ukrainian word.
        assert!(similarity("двері", "дверi") > 0.7);
    }

    #[test]
    fn distance_is_symmetric() {
        assert_eq!(
            distance_within("opens", "opened", 3),
            distance_within("opened", "opens", 3)
        );
    }

    #[test]
    fn window_similarity_finds_the_phrase_inside_a_sentence() {
        let haystack = tokens("the knight suddenly swings his sword at you");
        let needle = tokens("swings his sword");

        assert!((best_window_similarity(&haystack, &needle) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn window_similarity_tolerates_a_mistranscribed_word() {
        let haystack = tokens("the knight swings his swore at you");
        let needle = tokens("swings his sword");

        let score = best_window_similarity(&haystack, &needle);
        assert!((0.8..1.0).contains(&score), "got {score}");
    }

    #[test]
    fn window_similarity_rejects_an_unrelated_sentence() {
        let haystack = tokens("rain falls on the stone courtyard");
        let needle = tokens("swings his sword");

        assert!(best_window_similarity(&haystack, &needle) < 0.4);
    }

    #[test]
    fn a_needle_longer_than_the_haystack_scores_zero() {
        assert_eq!(
            best_window_similarity(&tokens("door"), &tokens("opens the door")),
            0.0
        );
        assert_eq!(best_window_similarity(&tokens("door"), &[]), 0.0);
    }
}
