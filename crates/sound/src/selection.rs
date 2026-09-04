//! Choosing which file to play when an event fires.
//!
//! Pure logic, deliberately free of kira and the filesystem: this is where the
//! "which door creak?" decision lives, and it must be testable without audio hardware.

use serde::{Deserialize, Serialize};

use crate::rng::Rng;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    /// Every enabled sound equally likely.
    #[default]
    Random,
    /// Likelihood proportional to each sound's weight.
    Weighted,
    /// Cycle through the group in order.
    Sequential,
}

/// One candidate file inside a group, reduced to only what selection needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub sound_id: i64,
    pub weight: f32,
}

impl Candidate {
    pub fn new(sound_id: i64, weight: f32) -> Self {
        Self { sound_id, weight }
    }
}

/// Selection state for a single sound group.
///
/// Holds the memory that makes repeats avoidable: the last sound played, and the
/// cursor for sequential mode.
#[derive(Debug, Clone)]
pub struct GroupSelector {
    mode: SelectionMode,
    prevent_repeat: bool,
    last_played: Option<i64>,
    cursor: usize,
}

impl GroupSelector {
    pub fn new(mode: SelectionMode, prevent_repeat: bool) -> Self {
        Self {
            mode,
            prevent_repeat,
            last_played: None,
            cursor: 0,
        }
    }

    pub fn mode(&self) -> SelectionMode {
        self.mode
    }

    pub fn last_played(&self) -> Option<i64> {
        self.last_played
    }

    /// Pick the next sound, or `None` if the group is empty.
    ///
    /// Anti-repeat only applies when there is something else to play: a group holding a
    /// single sound still plays that sound rather than falling silent.
    pub fn select(&mut self, candidates: &[Candidate], rng: &mut Rng) -> Option<i64> {
        if candidates.is_empty() {
            return None;
        }

        let pool: Vec<Candidate> = if self.prevent_repeat && candidates.len() > 1 {
            candidates
                .iter()
                .copied()
                .filter(|c| Some(c.sound_id) != self.last_played)
                .collect()
        } else {
            candidates.to_vec()
        };

        // The filter can empty the pool only if every candidate was the last played,
        // which cannot happen for distinct ids — but fall back rather than panic.
        let pool = if pool.is_empty() {
            candidates.to_vec()
        } else {
            pool
        };

        let chosen = match self.mode {
            SelectionMode::Random => pool[rng.below(pool.len())].sound_id,
            SelectionMode::Weighted => weighted_pick(&pool, rng),
            SelectionMode::Sequential => {
                // The cursor indexes the original group so the running order stays
                // stable even when anti-repeat removes an entry from the pool.
                let start = self.cursor % candidates.len();
                let picked = (0..candidates.len())
                    .map(|offset| candidates[(start + offset) % candidates.len()])
                    .find(|c| pool.iter().any(|p| p.sound_id == c.sound_id))
                    .unwrap_or(candidates[start]);

                let position = candidates
                    .iter()
                    .position(|c| c.sound_id == picked.sound_id)
                    .unwrap_or(start);
                self.cursor = position + 1;
                picked.sound_id
            }
        };

        self.last_played = Some(chosen);
        Some(chosen)
    }

    /// Forget the play history, e.g. after the group's contents change.
    pub fn reset(&mut self) {
        self.last_played = None;
        self.cursor = 0;
    }
}

fn weighted_pick(pool: &[Candidate], rng: &mut Rng) -> i64 {
    // Negative or non-finite weights are treated as zero rather than corrupting the
    // total; a group where every weight is zero degrades to uniform choice.
    let total: f32 = pool
        .iter()
        .map(|c| {
            if c.weight.is_finite() && c.weight > 0.0 {
                c.weight
            } else {
                0.0
            }
        })
        .sum();

    if total <= 0.0 {
        return pool[rng.below(pool.len())].sound_id;
    }

    let mut point = rng.next_f32() * total;
    for candidate in pool {
        let weight = if candidate.weight.is_finite() && candidate.weight > 0.0 {
            candidate.weight
        } else {
            0.0
        };
        point -= weight;
        if point <= 0.0 {
            return candidate.sound_id;
        }
    }

    // Floating point can leave a sliver unconsumed; the last positive entry wins.
    pool.iter()
        .rev()
        .find(|c| c.weight > 0.0)
        .unwrap_or(&pool[pool.len() - 1])
        .sound_id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(ids: &[i64]) -> Vec<Candidate> {
        ids.iter().map(|&id| Candidate::new(id, 1.0)).collect()
    }

    #[test]
    fn an_empty_group_selects_nothing() {
        let mut selector = GroupSelector::new(SelectionMode::Random, true);
        assert_eq!(selector.select(&[], &mut Rng::from_seed(1)), None);
    }

    #[test]
    fn a_single_sound_group_still_plays_despite_anti_repeat() {
        let mut selector = GroupSelector::new(SelectionMode::Random, true);
        let mut rng = Rng::from_seed(1);
        let candidates = group(&[7]);

        assert_eq!(selector.select(&candidates, &mut rng), Some(7));
        assert_eq!(selector.select(&candidates, &mut rng), Some(7));
    }

    #[test]
    fn anti_repeat_never_plays_the_same_sound_twice_in_a_row() {
        let mut selector = GroupSelector::new(SelectionMode::Random, true);
        let mut rng = Rng::from_seed(2024);
        let candidates = group(&[1, 2, 3]);

        let mut previous = None;
        for _ in 0..500 {
            let chosen = selector.select(&candidates, &mut rng).expect("a sound");
            assert_ne!(Some(chosen), previous, "repeated immediately");
            previous = Some(chosen);
        }
    }

    #[test]
    fn without_anti_repeat_immediate_repeats_are_possible() {
        let mut selector = GroupSelector::new(SelectionMode::Random, false);
        let mut rng = Rng::from_seed(5);
        let candidates = group(&[1, 2]);

        let mut previous = None;
        let repeated = (0..200).any(|_| {
            let chosen = selector.select(&candidates, &mut rng);
            let repeat = chosen == previous;
            previous = chosen;
            repeat
        });
        assert!(
            repeated,
            "expected at least one immediate repeat in 200 draws"
        );
    }

    #[test]
    fn random_selection_eventually_reaches_every_sound() {
        let mut selector = GroupSelector::new(SelectionMode::Random, true);
        let mut rng = Rng::from_seed(11);
        let candidates = group(&[10, 20, 30, 40]);

        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            seen.insert(selector.select(&candidates, &mut rng).expect("a sound"));
        }
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn sequential_selection_cycles_in_order() {
        let mut selector = GroupSelector::new(SelectionMode::Sequential, false);
        let mut rng = Rng::from_seed(1);
        let candidates = group(&[1, 2, 3]);

        let played: Vec<i64> = (0..7)
            .map(|_| selector.select(&candidates, &mut rng).expect("a sound"))
            .collect();
        assert_eq!(played, vec![1, 2, 3, 1, 2, 3, 1]);
    }

    #[test]
    fn sequential_selection_skips_the_last_played_when_anti_repeat_is_on() {
        // A group of two in sequential order would alternate anyway; the interesting
        // case is that anti-repeat never forces a stall.
        let mut selector = GroupSelector::new(SelectionMode::Sequential, true);
        let mut rng = Rng::from_seed(1);
        let candidates = group(&[1, 2]);

        let played: Vec<i64> = (0..6)
            .map(|_| selector.select(&candidates, &mut rng).expect("a sound"))
            .collect();
        assert_eq!(played, vec![1, 2, 1, 2, 1, 2]);
    }

    #[test]
    fn weighted_selection_respects_the_weights() {
        let mut selector = GroupSelector::new(SelectionMode::Weighted, false);
        let mut rng = Rng::from_seed(1234);
        let candidates = vec![Candidate::new(1, 9.0), Candidate::new(2, 1.0)];

        let mut heavy = 0;
        for _ in 0..10_000 {
            if selector.select(&candidates, &mut rng) == Some(1) {
                heavy += 1;
            }
        }
        // Expect ~9000. A wide band keeps the test from being flaky while still
        // failing loudly if weights are ignored.
        assert!(
            (8_500..9_500).contains(&heavy),
            "heavy sound chosen {heavy} times"
        );
    }

    #[test]
    fn zero_and_invalid_weights_degrade_to_uniform_choice() {
        let mut selector = GroupSelector::new(SelectionMode::Weighted, false);
        let mut rng = Rng::from_seed(8);
        let candidates = vec![
            Candidate::new(1, 0.0),
            Candidate::new(2, f32::NAN),
            Candidate::new(3, -4.0),
        ];

        let mut seen = std::collections::HashSet::new();
        for _ in 0..500 {
            seen.insert(selector.select(&candidates, &mut rng).expect("a sound"));
        }
        assert_eq!(seen.len(), 3, "all three should be reachable");
    }

    #[test]
    fn reset_clears_the_history() {
        let mut selector = GroupSelector::new(SelectionMode::Sequential, false);
        let mut rng = Rng::from_seed(1);
        let candidates = group(&[1, 2, 3]);

        selector.select(&candidates, &mut rng);
        selector.select(&candidates, &mut rng);
        selector.reset();

        assert_eq!(selector.last_played(), None);
        assert_eq!(selector.select(&candidates, &mut rng), Some(1));
    }
}
