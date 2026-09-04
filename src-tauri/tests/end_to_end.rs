//! The MVP path, end to end: spoken narration produces the right sound.
//!
//! Everything except the microphone and the window is real — real Silero VAD, real
//! Whisper, the real detector, the real trigger rules loaded from a real database, and
//! the real audio engine playing a real file. The microphone is replaced by a WAV of
//! spoken narration, which is exactly what the recorded-audio test mode does.
//!
//! This is the automated form of the spec's acceptance checklist. It also runs with the
//! network off once the models are cached, which is the product's defining requirement.

use std::path::{Path, PathBuf};

use dndsound_detect::{AlwaysRoll, DetectionInput, Detector, TriggerEngine, TriggerRules};
use dndsound_lib::audio::AudioState;
use dndsound_lib::library::{import_directory, ImportOptions};
use dndsound_models::ModelStore;
use dndsound_pipeline::{
    Segmenter, SileroVad, SpeechRecognizer, SttConfig, VadConfig, VadEvent, FRAME_SAMPLES,
};
use dndsound_sound::Bus;
use dndsound_store::{AppSettings, Db};

fn assets(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets")
        .join(kind)
        .canonicalize()
        .expect("assets should exist")
}

fn read_wav(name: &str) -> Vec<f32> {
    let path = assets("test-audio").join(name);
    let mut reader = hound::WavReader::open(&path).expect("fixture should exist");
    reader
        .samples::<i16>()
        .map(|s| s.expect("sample") as f32 / i16::MAX as f32)
        .collect()
}

fn model_store() -> ModelStore {
    ModelStore::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-models"))
}

/// A database set up the way a Dungeon Master would set it up: sounds imported, a group
/// of door creaks, and `OPEN_DOOR` pointing at that group.
fn prepared_database() -> Db {
    let db = Db::open_in_memory().expect("database");
    db.events().seed_if_empty().expect("seed events");

    let report = import_directory(
        &db,
        &assets("dev-sounds"),
        &ImportOptions {
            managed: false,
            library_dir: std::env::temp_dir().join("dndsound-e2e"),
        },
    )
    .expect("import sounds");
    assert!(!report.imported.is_empty(), "the dev sounds should import");

    let group = db.sounds().create_group("Wooden Doors").expect("group");
    for sound in report
        .imported
        .iter()
        .filter(|s| s.file_path.contains("door_wood_creak"))
    {
        db.sounds()
            .add_to_group(group.id, sound.id)
            .expect("add to group");
    }

    db.events()
        .set_sound_group("OPEN_DOOR", Some(group.id))
        .expect("assign the group to the event");

    db
}

/// Run a WAV through voice activity detection and speech recognition, exactly as the
/// listening session does.
fn transcribe_fixture(name: &str) -> Option<String> {
    let store = model_store();

    let vad_path = match store.ensure("silero-vad-16k", |_| {}) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("skipping: no VAD model ({e})");
            return None;
        }
    };
    let speech_path = match store.ensure("large-v3-turbo-q5_0", |_| {}) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("skipping: no speech model ({e})");
            return None;
        }
    };

    let mut vad = SileroVad::load(vad_path).expect("VAD loads");
    let recognizer =
        SpeechRecognizer::load(speech_path, SttConfig::default()).expect("speech model loads");
    let mut segmenter = Segmenter::new(VadConfig::default());

    let samples = read_wav(name);
    let mut segments = Vec::new();

    for frame in samples.chunks_exact(FRAME_SAMPLES) {
        let probability = vad.probability(frame).expect("inference");
        if let VadEvent::SegmentReady(segment) = segmenter.push(frame, probability) {
            segments.push(segment);
        }
    }
    if let Some(segment) = segmenter.flush() {
        segments.push(segment);
    }

    assert_eq!(
        segments.len(),
        1,
        "one spoken sentence should yield one segment"
    );

    let transcript = recognizer
        .transcribe(&segments[0].samples, false)
        .expect("transcription");
    assert!(
        recognizer.is_trustworthy(&transcript),
        "the transcript should be believable: {transcript:?}"
    );

    Some(transcript.text)
}

#[test]
fn ukrainian_narration_opens_a_door_and_plays_a_door_sound() {
    let Some(transcript) = transcribe_fixture("uk_open_door.wav") else {
        return;
    };
    println!("transcript: {transcript:?}");

    let db = prepared_database();

    // --- detection -------------------------------------------------------
    let stored = db.events().list().expect("events");
    let rules: Vec<TriggerRules> = stored
        .iter()
        .map(|event| TriggerRules::new(&event.definition.id, event.definition.cooldown_ms))
        .collect();
    let detector = Detector::new(stored.into_iter().map(|e| e.definition).collect());

    let detection = detector.detect(DetectionInput::final_transcript(&transcript, 0));
    let best = detection
        .best()
        .unwrap_or_else(|| panic!("nothing fired for {transcript:?}"));

    println!(
        "detected {} at {:.2} via {:?}",
        best.event_id, best.confidence, best.layer
    );
    assert_eq!(best.event_id, "OPEN_DOOR");
    assert!(best.confidence >= best.threshold);

    // --- trigger decision ------------------------------------------------
    let mut triggers = TriggerEngine::new();
    let decision = triggers.decide(&detection, &rules, &mut AlwaysRoll);
    assert_eq!(decision.triggers.len(), 1);
    assert_eq!(decision.triggers[0].event_id, "OPEN_DOOR");

    // --- playback --------------------------------------------------------
    let audio = AudioState::initialize(&AppSettings {
        master_volume: 0.02,
        ..AppSettings::default()
    });
    if !audio.snapshot().available {
        eprintln!("no audio output; detection was verified, playback was not");
        return;
    }

    let event = db.events().get("OPEN_DOOR").expect("event");
    let group_id = event.sound_group_id.expect("a group is assigned");
    let group = db.sounds().group(group_id).expect("group");
    let members = db.sounds().group_members(group_id).expect("members");

    let played = audio
        .play_group(&group, &members, Bus::Sfx)
        .expect("playback should succeed")
        .expect("the group has playable sounds");

    println!("played: {}", played.display_name);
    assert!(
        played.file_path.contains("door"),
        "a door event should play a door sound, got {}",
        played.file_path
    );
    assert_eq!(audio.snapshot().active_one_shots, 1);
}

#[test]
fn english_narration_takes_the_same_path() {
    let Some(transcript) = transcribe_fixture("en_open_door.wav") else {
        return;
    };
    println!("transcript: {transcript:?}");

    let db = prepared_database();
    let stored = db.events().list().expect("events");
    let detector = Detector::new(stored.into_iter().map(|e| e.definition).collect());

    let detection = detector.detect(DetectionInput::final_transcript(&transcript, 0));
    assert_eq!(
        detection.best().map(|c| c.event_id.as_str()),
        Some("OPEN_DOOR"),
        "English narration should reach the same event as Ukrainian"
    );
}

#[test]
fn narration_about_a_sword_on_a_table_plays_nothing() {
    let Some(transcript) = transcribe_fixture("en_no_action.wav") else {
        return;
    };
    println!("transcript: {transcript:?}");

    let db = prepared_database();
    let stored = db.events().list().expect("events");
    let detector = Detector::new(stored.into_iter().map(|e| e.definition).collect());

    let detection = detector.detect(DetectionInput::final_transcript(&transcript, 0));

    assert!(
        detection.best().is_none(),
        "a described sword must not trigger a swing: {:?}",
        detection.best()
    );

    // And the rejection is explained, which is what Debug Mode shows.
    let sword = detection
        .candidates
        .iter()
        .find(|candidate| candidate.event_id == "SWORD_SWING");
    if let Some(sword) = sword {
        println!(
            "rejected SWORD_SWING: {}",
            sword.rejection.as_ref().expect("a reason").explain()
        );
    }
}

#[test]
fn the_same_sentence_does_not_fire_twice() {
    let Some(transcript) = transcribe_fixture("uk_open_door.wav") else {
        return;
    };

    let db = prepared_database();
    let stored = db.events().list().expect("events");
    let rules: Vec<TriggerRules> = stored
        .iter()
        .map(|event| TriggerRules::new(&event.definition.id, event.definition.cooldown_ms))
        .collect();
    let detector = Detector::new(stored.into_iter().map(|e| e.definition).collect());

    let mut triggers = TriggerEngine::new();

    // The partial transcript fires first, then the final one carries the same phrase.
    let partial = detector.detect(DetectionInput::partial(&transcript, 0));
    assert_eq!(
        triggers
            .decide(&partial, &rules, &mut AlwaysRoll)
            .triggers
            .len(),
        1
    );

    let settled = detector.detect(DetectionInput::final_transcript(&transcript, 300));
    let decision = triggers.decide(&settled, &rules, &mut AlwaysRoll);

    assert!(
        decision.triggers.is_empty(),
        "the door should creak once, not twice"
    );
    assert!(
        !decision.suppressed.is_empty(),
        "and the suppression is recorded"
    );
}
