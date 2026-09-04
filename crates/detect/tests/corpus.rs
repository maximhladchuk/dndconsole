//! The corpus that guards detection quality.
//!
//! Realistic Dungeon Master narration in both languages, each line labelled with the
//! event it should fire — or with nothing, which is the more important half. A missed
//! sound costs far less than a wrong one, so **precision is reported separately from
//! recall and is not allowed to regress**.
//!
//! Runs in milliseconds with no microphone, no models and no audio hardware, which is
//! the entire reason `dndsound-detect` is a pure crate.

use dndsound_detect::corpus::{NEGATIVE, POSITIVE};
use dndsound_detect::{seed_events, DetectionInput, Detector};

fn detector() -> Detector {
    Detector::new(seed_events())
}

fn fired_event(detector: &Detector, text: &str) -> Option<String> {
    detector
        .detect(DetectionInput::final_transcript(text, 0))
        .best()
        .map(|candidate| candidate.event_id.clone())
}

#[test]
fn narration_fires_the_event_it_describes() {
    let detector = detector();
    let mut missed = Vec::new();

    for case in POSITIVE {
        let expected = case.expected.expect("positive cases have an expectation");
        match fired_event(&detector, case.text) {
            Some(fired) if fired == expected => {}
            other => missed.push((case.text, expected, other)),
        }
    }

    let recall = (POSITIVE.len() - missed.len()) as f32 / POSITIVE.len() as f32;
    println!(
        "recall: {:.0}% ({}/{})",
        recall * 100.0,
        POSITIVE.len() - missed.len(),
        POSITIVE.len()
    );
    for (text, expected, got) in &missed {
        println!("  MISSED {text:?} -> expected {expected}, got {got:?}");
    }

    assert!(
        recall >= 0.85,
        "recall dropped to {:.0}%; {} lines no longer fire",
        recall * 100.0,
        missed.len()
    );
}

#[test]
fn narration_without_an_event_stays_silent() {
    let detector = detector();
    let mut false_positives = Vec::new();

    for case in NEGATIVE {
        if let Some(fired) = fired_event(&detector, case.text) {
            false_positives.push((case.text, fired));
        }
    }

    for (text, fired) in &false_positives {
        println!("  FALSE POSITIVE {text:?} -> {fired}");
    }

    // Zero, not "few". This is the property the product lives or dies on.
    assert!(
        false_positives.is_empty(),
        "{} false positives: {false_positives:?}",
        false_positives.len()
    );
}

#[test]
fn the_specs_four_baseline_cases_behave() {
    let detector = detector();

    assert_eq!(
        fired_event(&detector, "The orc swings his sword.").as_deref(),
        Some("SWORD_SWING")
    );
    assert_eq!(fired_event(&detector, "A sword hangs on the wall."), None);
    assert_eq!(
        fired_event(&detector, "Він різко відчиняє двері.").as_deref(),
        Some("OPEN_DOOR")
    );
    assert_eq!(fired_event(&detector, "На дверях намальований вовк."), None);
}

#[test]
fn a_rejected_candidate_explains_itself() {
    let detector = detector();

    let detection = detector.detect(DetectionInput::final_transcript(
        "You see a sword lying on the table.",
        0,
    ));

    let sword = detection
        .candidates
        .iter()
        .find(|candidate| candidate.event_id == "SWORD_SWING")
        .expect("SWORD_SWING should be considered and then rejected");

    assert!(!sword.accepted);
    let reason = sword.rejection.as_ref().expect("a reason").explain();
    println!("rejection: {reason}");
    assert!(
        !reason.is_empty(),
        "Debug Mode needs a human-readable reason for every rejection"
    );
}

#[test]
fn a_spoken_command_fires_immediately_and_outranks_everything() {
    let detector = detector();

    let detection = detector.detect(DetectionInput::final_transcript("sound thunder", 0));
    let best = detection.best().expect("a command should fire");

    assert_eq!(best.event_id, "THUNDER");
    assert_eq!(best.confidence, 1.0);
    assert_eq!(best.layer, dndsound_detect::MatchLayer::Command);
}

#[test]
fn raising_sensitivity_never_costs_precision_on_this_corpus() {
    // Sensitivity is a user-facing slider. Turning it up should trade recall for
    // precision gradually, not open the floodgates.
    let mut detector = detector();
    detector.set_sensitivity(0.75);

    let false_positives: Vec<&str> = NEGATIVE
        .iter()
        .filter(|case| fired_event(&detector, case.text).is_some())
        .map(|case| case.text)
        .collect();

    assert!(
        false_positives.len() <= 1,
        "sensitivity 0.75 produced {} false positives: {false_positives:?}",
        false_positives.len()
    );
}

#[test]
fn detection_is_fast_enough_to_run_on_every_partial_transcript() {
    let detector = detector();

    let detection = detector.detect(DetectionInput::final_transcript(
        "The knight swings his sword at you while thunder rolls overhead.",
        0,
    ));

    // The number that matters is the release one — that is what runs at the table —
    // and it is roughly fifteen times faster than the unoptimised build. Asserting a
    // single figure across both would either be meaningless in release or fail every
    // `cargo test` in debug, so each build gets the budget it can be held to.
    let budget_us = if cfg!(debug_assertions) {
        25_000
    } else {
        2_000
    };

    println!(
        "detection took {} µs (budget {budget_us} µs for this build)",
        detection.elapsed_us
    );
    assert!(
        detection.elapsed_us < budget_us,
        "detection took {} µs, too slow to run per partial transcript",
        detection.elapsed_us
    );
}
