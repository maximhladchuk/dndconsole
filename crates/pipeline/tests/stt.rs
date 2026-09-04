//! Speech recognition against the real Whisper model and real speech.
//!
//! Needs `ggml-large-v3-turbo-q5_0.bin` (547 MB). It is downloaded on demand into
//! `target/test-models/` and cached; if that fails, the tests skip rather than fail.

use std::path::{Path, PathBuf};

use dndsound_models::ModelStore;
use dndsound_pipeline::{SpeechRecognizer, SttConfig};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/test-audio")
        .join(name)
}

fn read_wav(name: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(fixture(name)).expect("fixture should exist");
    reader
        .samples::<i16>()
        .map(|s| s.expect("sample") as f32 / i16::MAX as f32)
        .collect()
}

static MODEL: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

fn model_path() -> Option<&'static PathBuf> {
    MODEL
        .get_or_init(|| {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models");
            match ModelStore::new(dir).ensure("large-v3-turbo-q5_0", |_| {}) {
                Ok(path) => Some(path),
                Err(e) => {
                    eprintln!("skipping: could not obtain the speech model ({e})");
                    None
                }
            }
        })
        .as_ref()
}

fn recognizer(config: SttConfig) -> Option<SpeechRecognizer> {
    let path = model_path()?;
    Some(SpeechRecognizer::load(path, config).expect("the model should load"))
}

fn normalized(text: &str) -> String {
    text.to_lowercase()
}

#[test]
fn english_narration_is_transcribed() {
    let Some(recognizer) = recognizer(SttConfig::default()) else {
        return;
    };

    let transcript = recognizer
        .transcribe(&read_wav("en_open_door.wav"), false)
        .expect("transcription should succeed");

    println!(
        "english: {:?} ({} ms)",
        transcript.text, transcript.elapsed_ms
    );

    assert!(recognizer.is_trustworthy(&transcript));
    let text = normalized(&transcript.text);
    assert!(
        text.contains("door"),
        "expected the door to be mentioned: {text}"
    );
    assert!(
        text.contains("open"),
        "expected opening to be mentioned: {text}"
    );
}

#[test]
fn ukrainian_narration_is_transcribed() {
    let Some(recognizer) = recognizer(SttConfig::default()) else {
        return;
    };

    let transcript = recognizer
        .transcribe(&read_wav("uk_open_door.wav"), false)
        .expect("transcription should succeed");

    println!(
        "ukrainian: {:?} (lang {:?}, {} ms)",
        transcript.text, transcript.language, transcript.elapsed_ms
    );

    assert!(recognizer.is_trustworthy(&transcript));
    let text = normalized(&transcript.text);
    assert!(
        text.contains("двер"),
        "expected the door to be mentioned in Ukrainian: {text}"
    );
    assert_eq!(
        transcript.language.as_deref(),
        Some("uk"),
        "language detection should identify Ukrainian"
    );
}

/// The synthetic voice slurs "дістає меч" badly enough that whisper writes
/// "дістаємеш", so this asserts the sentence is recognisably about a goblin rather than
/// demanding an exact match. Transcription accuracy on real narration is a tuning
/// question, measured against real recordings — not against text-to-speech.
#[test]
fn ukrainian_combat_narration_is_recognisably_about_the_goblin() {
    let Some(recognizer) = recognizer(SttConfig::default()) else {
        return;
    };

    let transcript = recognizer
        .transcribe(&read_wav("uk_sword.wav"), false)
        .expect("transcription should succeed");

    println!("ukrainian combat: {:?}", transcript.text);
    let text = normalized(&transcript.text);
    assert!(
        text.contains("гобл"),
        "expected recognisable narration: {text}"
    );
}

#[test]
fn silence_is_rejected_rather_than_hallucinated_into_words() {
    let Some(recognizer) = recognizer(SttConfig::default()) else {
        return;
    };

    let transcript = recognizer
        .transcribe(&read_wav("silence.wav"), false)
        .expect("transcription should not error on silence");

    println!(
        "silence: {:?} (no_speech {:.2})",
        transcript.text, transcript.no_speech_probability
    );

    assert!(
        !recognizer.is_trustworthy(&transcript),
        "silence must never produce a transcript the pipeline believes"
    );
}

/// The prompted path, which is what the application runs whenever a language is picked.
///
/// A prompt is fed to the decoder as the previous sentence, and whisper is entirely
/// willing to decide the previous sentence simply continued. A bilingual prompt did
/// exactly that here — silence came back as "The cleric heals your wounds", which would
/// have played a healing sound into a quiet room.
#[test]
fn silence_is_still_rejected_when_a_vocabulary_prompt_is_used() {
    for language in ["uk", "en"] {
        let Some(recognizer) = recognizer(SttConfig::for_language(Some(language))) else {
            return;
        };

        let transcript = recognizer
            .transcribe(&read_wav("silence.wav"), false)
            .expect("transcription should not error on silence");

        println!(
            "silence with the {language} prompt: {:?} (no_speech {:.2})",
            transcript.text, transcript.no_speech_probability
        );
        assert!(
            !recognizer.is_trustworthy(&transcript),
            "silence with the {language} prompt produced a transcript the pipeline believes: {:?}",
            transcript.text
        );
    }
}

/// The prompt exists to fix this exact sentence: without one, whisper writes "дістає
/// **меж** і різко **біє**".
#[test]
fn a_prompted_decode_spells_the_fantasy_words() {
    let Some(recognizer) = recognizer(SttConfig::for_language(Some("uk"))) else {
        return;
    };

    let transcript = recognizer
        .transcribe(&read_wav("uk_sword.wav"), false)
        .expect("transcription should succeed");

    println!("prompted ukrainian combat: {:?}", transcript.text);
    let text = normalized(&transcript.text);

    // Cyrillic, not "Goblin" — a bilingual prompt switched scripts mid-sentence, and no
    // term list would ever match that.
    assert!(text.contains("гобл"), "{text}");
    assert!(
        text.contains("меч"),
        "expected 'меч', not 'меж' or 'меш': {text}"
    );
}

#[test]
fn pinning_the_language_still_transcribes_that_language() {
    let Some(recognizer) = recognizer(SttConfig {
        language: Some("uk".to_string()),
        ..SttConfig::default()
    }) else {
        return;
    };

    let transcript = recognizer
        .transcribe(&read_wav("uk_sword.wav"), false)
        .expect("transcription should succeed");

    assert!(!transcript.is_empty());
    println!("pinned Ukrainian: {:?}", transcript.text);
}

#[test]
fn trimming_the_audio_context_is_faster_and_keeps_the_words() {
    let Some(trimmed) = recognizer(SttConfig::default()) else {
        return;
    };
    let Some(untrimmed) = recognizer(SttConfig {
        trim_audio_context: false,
        ..SttConfig::default()
    }) else {
        return;
    };

    let samples = read_wav("en_open_door.wav");

    // Warm up so neither timing pays for first-run allocation.
    let _ = trimmed.transcribe(&samples, false);

    let with_trim = trimmed.transcribe(&samples, false).expect("transcribe");
    let without_trim = untrimmed.transcribe(&samples, false).expect("transcribe");

    println!(
        "audio_ctx trimmed: {} ms, untrimmed: {} ms",
        with_trim.elapsed_ms, without_trim.elapsed_ms
    );

    assert!(
        normalized(&with_trim.text).contains("door"),
        "trimming must not cost us the words: {:?}",
        with_trim.text
    );
    assert!(
        with_trim.elapsed_ms <= without_trim.elapsed_ms,
        "trimming should not be slower: {} ms vs {} ms",
        with_trim.elapsed_ms,
        without_trim.elapsed_ms
    );
}

#[test]
fn transcribing_nothing_is_an_error_rather_than_an_empty_success() {
    let Some(recognizer) = recognizer(SttConfig::default()) else {
        return;
    };
    assert!(recognizer.transcribe(&[], false).is_err());
}
