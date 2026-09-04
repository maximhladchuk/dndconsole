//! Measure what the example phrases are worth, by deleting them.
//!
//! Answers a question that is not answerable by reading the editor: are the example
//! phrases matching material, or documentation?
//!
//! Note what the corpus alone cannot tell you. Half the positive cases *are* the example
//! phrases, so measuring against them measures whether a list can read itself. The
//! `FRESH` lines below were written without looking at any term list, and they are the
//! ones worth watching.
//!
//! There is no semantic layer here — this binary is pure and loads no model — so the
//! numbers are the floor: whatever the phrases are worth on top of this, they earn
//! through the semantic index, which is built from nothing else.

use dndsound_detect::corpus::{NEGATIVE, POSITIVE};
use dndsound_detect::{seed_events, DetectionInput, Detector};

fn score(detector: &Detector, label: &str) {
    let mut hits = 0;
    let mut by_layer = std::collections::BTreeMap::new();

    for case in POSITIVE {
        let detection = detector.detect(DetectionInput::final_transcript(case.text, 0));
        if let Some(best) = detection.best() {
            if Some(best.event_id.as_str()) == case.expected {
                hits += 1;
                *by_layer.entry(format!("{:?}", best.layer)).or_insert(0) += 1;
            }
        }
    }

    let false_positives = NEGATIVE
        .iter()
        .filter(|case| {
            detector
                .detect(DetectionInput::final_transcript(case.text, 0))
                .best()
                .is_some()
        })
        .count();

    println!(
        "{label:<22} recall {hits}/{}  false positives {false_positives}",
        POSITIVE.len()
    );
    for (layer, count) in by_layer {
        println!("    {layer:<12} {count}");
    }
}

/// Lines written without looking at any term list — the only honest measure, since half
/// the corpus positives *are* the example phrases and will always match them.
const FRESH: &[(&str, &str)] = &[
    ("Маг метнув фаєрбол у натовп.", "FIREBALL"),
    ("Цілителька заживила його рани.", "SPELL_HEAL"),
    ("Крига скувала озеро.", "SPELL_ICE"),
    ("Лицар вихопив шаблю з піхов.", "SWORD_UNSHEATHE"),
    ("Вона підставила щит під сокиру.", "SHIELD_BLOCK"),
    ("Лати гримнули, коли він розвернувся.", "ARMOR_CLANK"),
    ("Орк повалився на землю.", "BODY_FALL"),
    ("Хтось заволав у сусідній кімнаті.", "SCREAM"),
    ("Дракон заревів так, що посипалося каміння.", "DRAGON_ROAR"),
    ("Коні зацокотіли по бруківці.", "HORSE"),
    ("Дух застогнав у підземеллі.", "GHOST_MOAN"),
    ("Череп хруснув під чоботом.", "BONES"),
    ("Круки закружляли над трупом.", "CROW"),
    ("Він осушив флакон одним ковтком.", "POTION_DRINK"),
    ("Замок клацнув і піддався.", "KEYS_LOCK"),
    ("Вона розгорнула пергамент на колінах.", "SCROLL_PAPER"),
    ("Вітер завив між скель.", "WIND"),
    ("Дощ полив як з відра.", "RAIN"),
    ("Дзвони забили на сполох.", "BELL"),
];

fn fresh(detector: &Detector, label: &str) {
    let hits = FRESH
        .iter()
        .filter(|(text, expected)| {
            detector
                .detect(DetectionInput::final_transcript(text, 0))
                .best()
                .map(|c| c.event_id == *expected)
                .unwrap_or(false)
        })
        .count();
    println!("{label:<22} fresh lines {hits}/{}", FRESH.len());
}

fn main() {
    score(&Detector::new(seed_events()), "as shipped");
    fresh(&Detector::new(seed_events()), "as shipped");

    let stripped: Vec<_> = seed_events()
        .into_iter()
        .map(|mut event| {
            // Keep the spoken commands; those are a separate feature.
            event.phrases.retain(|phrase| phrase.is_command);
            event
        })
        .collect();
    score(&Detector::new(stripped.clone()), "no example phrases");
    fresh(&Detector::new(stripped), "no example phrases");

    // And the reverse: nothing but the phrases.
    let terms_only: Vec<_> = seed_events()
        .into_iter()
        .map(|mut event| {
            event.terms.clear();
            event
        })
        .collect();
    score(&Detector::new(terms_only.clone()), "no terms");
    fresh(&Detector::new(terms_only), "no terms");
}
