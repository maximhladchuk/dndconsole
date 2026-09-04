//! Deciding whether an accepted candidate actually fires.
//!
//! Detection says "this narration describes a sword swing". This module says whether to
//! play it *now*, which is a different question: the same sentence gets re-transcribed
//! as it is being spoken, the word "sword" appears twice in one breath, and the same
//! event fires three times in ten seconds.
//!
//! Pure and clock-injected — time is passed in, never read — so every rule here is
//! testable to the millisecond.

use std::collections::VecDeque;

use serde::Serialize;

use crate::engine::{Candidate, Detection};

/// How long a triggering phrase is remembered for duplicate suppression.
const SPAN_MEMORY_MS: i64 = 8_000;
/// How many recent triggers to keep. Bounded so a long session cannot grow memory.
const SPAN_MEMORY_LIMIT: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub event_id: String,
    pub confidence: f32,
    pub at_ms: i64,
    /// Milliseconds to wait before playing, for chained events.
    pub delay_ms: u32,
    pub transcript: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "reason",
    content = "detail"
)]
pub enum SuppressionReason {
    /// The event fired recently and is still cooling down.
    Cooldown { remaining_ms: i64 },
    /// The same phrase already fired this event — usually a partial transcript being
    /// re-detected once it settles.
    DuplicateSpan { span: String },
    /// The probability roll went against it.
    Probability { probability: f32 },
}

impl SuppressionReason {
    pub fn explain(&self) -> String {
        match self {
            SuppressionReason::Cooldown { remaining_ms } => {
                format!("still cooling down for another {remaining_ms} ms")
            }
            SuppressionReason::DuplicateSpan { span } => {
                format!("'{span}' already triggered this event")
            }
            SuppressionReason::Probability { probability } => {
                format!(
                    "the {:.0}% probability roll went against it",
                    probability * 100.0
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suppressed {
    pub event_id: String,
    pub confidence: f32,
    pub reason: SuppressionReason,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub triggers: Vec<Trigger>,
    pub suppressed: Vec<Suppressed>,
}

/// Per-event rules the trigger engine needs. Kept separate from `EventDefinition` so the
/// engine does not need the whole definition on the hot path.
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerRules {
    pub event_id: String,
    pub cooldown_ms: u32,
    pub probability: f32,
    /// Events to fire after this one, and how long to wait.
    pub followers: Vec<(String, u32)>,
}

impl TriggerRules {
    pub fn new(event_id: &str, cooldown_ms: u32) -> Self {
        Self {
            event_id: event_id.to_string(),
            cooldown_ms,
            probability: 1.0,
            followers: Vec::new(),
        }
    }

    pub fn with_probability(mut self, probability: f32) -> Self {
        self.probability = probability;
        self
    }

    pub fn with_follower(mut self, event_id: &str, delay_ms: u32) -> Self {
        self.followers.push((event_id.to_string(), delay_ms));
        self
    }
}

/// Anything that can produce a number in `0.0..1.0`. Injected so probability is
/// deterministic in tests.
pub trait Roll {
    fn next(&mut self) -> f32;
}

/// Always fires. The default when probability is not in play.
pub struct AlwaysRoll;

impl Roll for AlwaysRoll {
    fn next(&mut self) -> f32 {
        0.0
    }
}

struct RecentSpan {
    event_id: String,
    span: String,
    at_ms: i64,
}

/// Turns detections into playback decisions.
#[derive(Default)]
pub struct TriggerEngine {
    last_fired: Vec<(String, i64)>,
    recent_spans: VecDeque<RecentSpan>,
}

impl TriggerEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget everything. Used when a session starts.
    pub fn reset(&mut self) {
        self.last_fired.clear();
        self.recent_spans.clear();
    }

    pub fn decide(
        &mut self,
        detection: &Detection,
        rules: &[TriggerRules],
        roll: &mut impl Roll,
    ) -> Decision {
        let now = detection.timestamp_ms;
        self.forget_old_spans(now);

        let mut triggers = Vec::new();
        let mut suppressed = Vec::new();

        for candidate in detection.accepted() {
            let Some(rule) = rules
                .iter()
                .find(|rule| rule.event_id == candidate.event_id)
            else {
                continue;
            };

            if let Some(reason) = self.suppression_for(candidate, rule, now) {
                suppressed.push(Suppressed {
                    event_id: candidate.event_id.clone(),
                    confidence: candidate.confidence,
                    reason,
                });
                continue;
            }

            if rule.probability < 1.0 && roll.next() >= rule.probability {
                suppressed.push(Suppressed {
                    event_id: candidate.event_id.clone(),
                    confidence: candidate.confidence,
                    reason: SuppressionReason::Probability {
                        probability: rule.probability,
                    },
                });
                continue;
            }

            self.remember(candidate, now);

            triggers.push(Trigger {
                event_id: candidate.event_id.clone(),
                confidence: candidate.confidence,
                at_ms: now,
                delay_ms: 0,
                transcript: detection.transcript.clone(),
            });

            // Chained events ride on the same decision, so they still respect the
            // cooldowns of the events they chain to.
            for (follower_id, delay_ms) in &rule.followers {
                let follower_rule = rules.iter().find(|r| &r.event_id == follower_id);
                let cooling = follower_rule
                    .map(|rule| self.cooldown_remaining(&rule.event_id, rule.cooldown_ms, now))
                    .unwrap_or(0);

                if cooling > 0 {
                    suppressed.push(Suppressed {
                        event_id: follower_id.clone(),
                        confidence: candidate.confidence,
                        reason: SuppressionReason::Cooldown {
                            remaining_ms: cooling,
                        },
                    });
                    continue;
                }

                self.mark_fired(follower_id, now + i64::from(*delay_ms));
                triggers.push(Trigger {
                    event_id: follower_id.clone(),
                    confidence: candidate.confidence,
                    at_ms: now,
                    delay_ms: *delay_ms,
                    transcript: detection.transcript.clone(),
                });
            }
        }

        Decision {
            triggers,
            suppressed,
        }
    }

    fn suppression_for(
        &self,
        candidate: &Candidate,
        rule: &TriggerRules,
        now: i64,
    ) -> Option<SuppressionReason> {
        let remaining = self.cooldown_remaining(&candidate.event_id, rule.cooldown_ms, now);
        if remaining > 0 {
            return Some(SuppressionReason::Cooldown {
                remaining_ms: remaining,
            });
        }

        let span = span_key(candidate);
        if self
            .recent_spans
            .iter()
            .any(|recent| recent.event_id == candidate.event_id && recent.span == span)
        {
            return Some(SuppressionReason::DuplicateSpan { span });
        }

        None
    }

    fn cooldown_remaining(&self, event_id: &str, cooldown_ms: u32, now: i64) -> i64 {
        self.last_fired
            .iter()
            .find(|(id, _)| id == event_id)
            .map(|(_, at)| i64::from(cooldown_ms) - (now - at))
            .filter(|remaining| *remaining > 0)
            .unwrap_or(0)
    }

    fn remember(&mut self, candidate: &Candidate, now: i64) {
        self.mark_fired(&candidate.event_id, now);

        self.recent_spans.push_back(RecentSpan {
            event_id: candidate.event_id.clone(),
            span: span_key(candidate),
            at_ms: now,
        });
        while self.recent_spans.len() > SPAN_MEMORY_LIMIT {
            self.recent_spans.pop_front();
        }
    }

    fn mark_fired(&mut self, event_id: &str, at_ms: i64) {
        if let Some(entry) = self.last_fired.iter_mut().find(|(id, _)| id == event_id) {
            entry.1 = at_ms;
        } else {
            self.last_fired.push((event_id.to_string(), at_ms));
        }
    }

    fn forget_old_spans(&mut self, now: i64) {
        while let Some(front) = self.recent_spans.front() {
            if now - front.at_ms > SPAN_MEMORY_MS {
                self.recent_spans.pop_front();
            } else {
                break;
            }
        }
    }
}

/// What counts as "the same trigger happening again".
///
/// The matched span rather than the whole transcript: a partial transcript and the final
/// one differ in their tails but share the phrase that fired the event, and firing twice
/// for one swing is exactly what must not happen.
fn span_key(candidate: &Candidate) -> String {
    candidate.matched_span.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Detection, MatchLayer};

    fn candidate(event_id: &str, span: &str) -> Candidate {
        Candidate {
            event_id: event_id.to_string(),
            confidence: 0.9,
            threshold: 0.82,
            layer: MatchLayer::ExactPhrase,
            matched_span: span.to_string(),
            accepted: true,
            rejection: None,
            action_word: None,
        }
    }

    fn detection(at_ms: i64, candidates: Vec<Candidate>) -> Detection {
        Detection {
            transcript: "the knight swings his sword".to_string(),
            normalized: "the knight swings his sword".to_string(),
            is_final: true,
            timestamp_ms: at_ms,
            candidates,
            elapsed_us: 0,
        }
    }

    /// A roll that returns a fixed sequence, so probability tests are deterministic.
    struct ScriptedRoll(std::vec::IntoIter<f32>);

    impl Roll for ScriptedRoll {
        fn next(&mut self) -> f32 {
            self.0.next().unwrap_or(0.0)
        }
    }

    #[test]
    fn an_accepted_candidate_fires() {
        let mut engine = TriggerEngine::new();
        let rules = vec![TriggerRules::new("SWORD_SWING", 1_500)];

        let decision = engine.decide(
            &detection(0, vec![candidate("SWORD_SWING", "swings his sword")]),
            &rules,
            &mut AlwaysRoll,
        );

        assert_eq!(decision.triggers.len(), 1);
        assert_eq!(decision.triggers[0].event_id, "SWORD_SWING");
        assert!(decision.suppressed.is_empty());
    }

    #[test]
    fn the_same_event_is_suppressed_during_its_cooldown() {
        let mut engine = TriggerEngine::new();
        let rules = vec![TriggerRules::new("SWORD_SWING", 1_500)];

        engine.decide(
            &detection(0, vec![candidate("SWORD_SWING", "swings his sword")]),
            &rules,
            &mut AlwaysRoll,
        );

        // A different phrase, 500 ms later — still inside the cooldown.
        let decision = engine.decide(
            &detection(500, vec![candidate("SWORD_SWING", "slashes at you")]),
            &rules,
            &mut AlwaysRoll,
        );

        assert!(decision.triggers.is_empty());
        assert!(matches!(
            decision.suppressed[0].reason,
            SuppressionReason::Cooldown { remaining_ms } if remaining_ms == 1_000
        ));
    }

    #[test]
    fn the_event_fires_again_once_the_cooldown_expires() {
        let mut engine = TriggerEngine::new();
        let rules = vec![TriggerRules::new("SWORD_SWING", 1_500)];

        engine.decide(
            &detection(0, vec![candidate("SWORD_SWING", "swings his sword")]),
            &rules,
            &mut AlwaysRoll,
        );
        let decision = engine.decide(
            &detection(1_600, vec![candidate("SWORD_SWING", "slashes at you")]),
            &rules,
            &mut AlwaysRoll,
        );

        assert_eq!(decision.triggers.len(), 1);
    }

    #[test]
    fn the_same_phrase_never_fires_twice_even_after_the_cooldown() {
        // "He swings his sword, and the sword cuts through the air" — one swing.
        let mut engine = TriggerEngine::new();
        let rules = vec![TriggerRules::new("SWORD_SWING", 100)];

        engine.decide(
            &detection(0, vec![candidate("SWORD_SWING", "swings his sword")]),
            &rules,
            &mut AlwaysRoll,
        );
        let decision = engine.decide(
            &detection(2_000, vec![candidate("SWORD_SWING", "swings his sword")]),
            &rules,
            &mut AlwaysRoll,
        );

        assert!(decision.triggers.is_empty());
        assert!(matches!(
            decision.suppressed[0].reason,
            SuppressionReason::DuplicateSpan { .. }
        ));
    }

    #[test]
    fn a_partial_transcript_firing_stops_the_final_one_firing_again() {
        // The realistic sequence: the partial fires early, the settled transcript
        // arrives a moment later carrying the same phrase.
        let mut engine = TriggerEngine::new();
        let rules = vec![TriggerRules::new("OPEN_DOOR", 2_500)];

        let mut partial = detection(0, vec![candidate("OPEN_DOOR", "opens the door")]);
        partial.is_final = false;
        assert_eq!(
            engine
                .decide(&partial, &rules, &mut AlwaysRoll)
                .triggers
                .len(),
            1
        );

        let final_transcript = detection(400, vec![candidate("OPEN_DOOR", "opens the door")]);
        assert!(engine
            .decide(&final_transcript, &rules, &mut AlwaysRoll)
            .triggers
            .is_empty());
    }

    #[test]
    fn a_forgotten_phrase_can_fire_again_much_later() {
        let mut engine = TriggerEngine::new();
        let rules = vec![TriggerRules::new("OPEN_DOOR", 1_000)];

        engine.decide(
            &detection(0, vec![candidate("OPEN_DOOR", "opens the door")]),
            &rules,
            &mut AlwaysRoll,
        );

        // Well past the span memory: a genuinely new door, later in the session.
        let decision = engine.decide(
            &detection(
                SPAN_MEMORY_MS + 1_000,
                vec![candidate("OPEN_DOOR", "opens the door")],
            ),
            &rules,
            &mut AlwaysRoll,
        );

        assert_eq!(decision.triggers.len(), 1);
    }

    #[test]
    fn probability_suppresses_some_triggers_and_says_so() {
        let mut engine = TriggerEngine::new();
        let rules = vec![TriggerRules::new("THUNDER", 0).with_probability(0.5)];

        // 0.9 is above the 0.5 probability, so this roll loses.
        let mut roll = ScriptedRoll(vec![0.9].into_iter());
        let decision = engine.decide(
            &detection(0, vec![candidate("THUNDER", "thunder rolls")]),
            &rules,
            &mut roll,
        );

        assert!(decision.triggers.is_empty());
        assert!(matches!(
            decision.suppressed[0].reason,
            SuppressionReason::Probability { .. }
        ));

        // 0.1 is below it, so this one wins.
        let mut roll = ScriptedRoll(vec![0.1].into_iter());
        let decision = engine.decide(
            &detection(1_000, vec![candidate("THUNDER", "thunder crashes")]),
            &rules,
            &mut roll,
        );
        assert_eq!(decision.triggers.len(), 1);
    }

    #[test]
    fn chained_events_fire_with_their_delay() {
        let mut engine = TriggerEngine::new();
        let rules = vec![
            TriggerRules::new("DRAW_SWORD", 1_000).with_follower("SWORD_SWING", 450),
            TriggerRules::new("SWORD_SWING", 1_000),
        ];

        let decision = engine.decide(
            &detection(0, vec![candidate("DRAW_SWORD", "draws his sword")]),
            &rules,
            &mut AlwaysRoll,
        );

        assert_eq!(decision.triggers.len(), 2);
        assert_eq!(decision.triggers[0].event_id, "DRAW_SWORD");
        assert_eq!(decision.triggers[0].delay_ms, 0);
        assert_eq!(decision.triggers[1].event_id, "SWORD_SWING");
        assert_eq!(decision.triggers[1].delay_ms, 450);
    }

    #[test]
    fn a_chained_event_still_respects_its_own_cooldown() {
        let mut engine = TriggerEngine::new();
        let rules = vec![
            TriggerRules::new("DRAW_SWORD", 0).with_follower("SWORD_SWING", 450),
            TriggerRules::new("SWORD_SWING", 5_000),
        ];

        engine.decide(
            &detection(0, vec![candidate("SWORD_SWING", "swings his sword")]),
            &rules,
            &mut AlwaysRoll,
        );

        let decision = engine.decide(
            &detection(500, vec![candidate("DRAW_SWORD", "draws his sword")]),
            &rules,
            &mut AlwaysRoll,
        );

        assert_eq!(decision.triggers.len(), 1, "only the parent should fire");
        assert_eq!(decision.triggers[0].event_id, "DRAW_SWORD");
        assert!(decision
            .suppressed
            .iter()
            .any(|s| s.event_id == "SWORD_SWING"));
    }

    #[test]
    fn candidates_without_rules_are_ignored_rather_than_firing_blind() {
        let mut engine = TriggerEngine::new();
        let decision = engine.decide(
            &detection(0, vec![candidate("UNKNOWN_EVENT", "whatever")]),
            &[],
            &mut AlwaysRoll,
        );

        assert!(decision.triggers.is_empty());
        assert!(decision.suppressed.is_empty());
    }

    #[test]
    fn rejected_candidates_never_reach_the_trigger_engine() {
        let mut engine = TriggerEngine::new();
        let mut rejected = candidate("SWORD_SWING", "sword");
        rejected.accepted = false;

        let decision = engine.decide(
            &detection(0, vec![rejected]),
            &[TriggerRules::new("SWORD_SWING", 0)],
            &mut AlwaysRoll,
        );

        assert!(decision.triggers.is_empty());
    }

    #[test]
    fn span_memory_stays_bounded_across_a_long_session() {
        let mut engine = TriggerEngine::new();
        let rules = vec![TriggerRules::new("OPEN_DOOR", 0)];

        for i in 0..1_000 {
            let at = i as i64 * 100;
            engine.decide(
                &detection(at, vec![candidate("OPEN_DOOR", &format!("phrase {i}"))]),
                &rules,
                &mut AlwaysRoll,
            );
        }

        assert!(
            engine.recent_spans.len() <= SPAN_MEMORY_LIMIT,
            "span memory grew to {}",
            engine.recent_spans.len()
        );
        assert_eq!(engine.last_fired.len(), 1);
    }

    #[test]
    fn resetting_clears_the_history() {
        let mut engine = TriggerEngine::new();
        let rules = vec![TriggerRules::new("OPEN_DOOR", 10_000)];

        engine.decide(
            &detection(0, vec![candidate("OPEN_DOOR", "opens the door")]),
            &rules,
            &mut AlwaysRoll,
        );
        engine.reset();

        let decision = engine.decide(
            &detection(100, vec![candidate("OPEN_DOOR", "opens the door")]),
            &rules,
            &mut AlwaysRoll,
        );
        assert_eq!(decision.triggers.len(), 1, "a reset session starts clean");
    }
}
