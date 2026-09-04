//! The JSON the frontend actually receives.
//!
//! `src/types/api.ts` is a hand-kept mirror of these types, so nothing on the Rust side
//! fails when the two drift — the UI just reads `undefined`. That is how `at_ms` reached
//! the transcript panel and rendered as "Invalid Date" while every test passed.
//!
//! The trap is specific: `#[serde(rename_all = "camelCase")]` on an *enum* renames the
//! variants and leaves the fields inside each variant alone. Struct variants need
//! `rename_all_fields` as well. These tests assert the property directly instead of
//! trusting the attribute.

use dndsound_detect::{
    Candidate, Decision, Detection, MatchLayer, RejectionReason, Suppressed, SuppressionReason,
    Trigger,
};
use serde_json::Value;

/// Every key anywhere in the value, however deeply nested.
fn keys(value: &Value, into: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                into.push(key.clone());
                keys(child, into);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keys(item, into)),
        _ => {}
    }
}

#[track_caller]
fn assert_camel_case<T: serde::Serialize>(value: &T) {
    let json = serde_json::to_value(value).expect("serialize");
    let mut found = Vec::new();
    keys(&json, &mut found);

    assert!(!found.is_empty(), "nothing to check in {json}");
    for key in found {
        assert!(
            !key.contains('_'),
            "'{key}' is snake_case; the TypeScript mirror expects camelCase. Full value: {json}"
        );
    }
}

fn candidate() -> Candidate {
    Candidate {
        event_id: "SWORD_SWING".to_string(),
        confidence: 0.91,
        threshold: 0.75,
        layer: MatchLayer::ExactPhrase,
        matched_span: "swings his sword".to_string(),
        accepted: true,
        rejection: None,
        action_word: Some("swings".to_string()),
    }
}

#[test]
fn detections_reach_the_ui_in_camel_case() {
    assert_camel_case(&Detection {
        transcript: "he swings his sword".to_string(),
        normalized: "he swings his sword".to_string(),
        is_final: true,
        timestamp_ms: 1_700_000_000_000,
        candidates: vec![candidate()],
        elapsed_us: 420,
    });
}

#[test]
fn every_rejection_reason_reaches_the_ui_in_camel_case() {
    // Struct variants are the ones at risk; the unit and newtype variants are here so
    // that adding a field to any of them is covered too.
    for reason in [
        RejectionReason::BelowThreshold {
            score: 0.6,
            threshold: 0.75,
        },
        RejectionReason::NegativePhrase("lying on the table".to_string()),
        RejectionReason::NoActionWord,
        RejectionReason::FramedAsMemoryOrHypothesis("remember".to_string()),
        RejectionReason::Disabled,
    ] {
        assert_camel_case(&reason);
    }
}

#[test]
fn every_suppression_reason_reaches_the_ui_in_camel_case() {
    for reason in [
        SuppressionReason::Cooldown { remaining_ms: 800 },
        SuppressionReason::DuplicateSpan {
            span: "swings his sword".to_string(),
        },
        SuppressionReason::Probability { probability: 0.5 },
    ] {
        assert_camel_case(&reason);
    }
}

#[test]
fn decisions_reach_the_ui_in_camel_case() {
    assert_camel_case(&Decision {
        triggers: vec![Trigger {
            event_id: "SWORD_SWING".to_string(),
            confidence: 0.91,
            at_ms: 1_700_000_000_000,
            delay_ms: 0,
            transcript: "he swings his sword".to_string(),
        }],
        suppressed: vec![Suppressed {
            event_id: "OPEN_DOOR".to_string(),
            confidence: 0.88,
            reason: SuppressionReason::Cooldown {
                remaining_ms: 1_200,
            },
        }],
    });
}

/// The exact shape `src/types/api.ts` declares for a cooldown suppression.
#[test]
fn a_cooldown_matches_the_typescript_mirror_field_for_field() {
    let json =
        serde_json::to_value(SuppressionReason::Cooldown { remaining_ms: 800 }).expect("serialize");

    assert_eq!(json["reason"], "cooldown");
    assert_eq!(json["detail"]["remainingMs"], 800);
}
