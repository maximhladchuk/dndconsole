//! Voice activity detection against the real Silero model and real speech.
//!
//! Downloads the model on first run (1.3 MB) and caches it; skips if that fails, so an
//! offline machine still runs the rest of the suite.

use std::path::{Path, PathBuf};

use dndsound_models::ModelStore;
use dndsound_pipeline::{Segmenter, SileroVad, VadConfig, VadEvent, FRAME_SAMPLES};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/test-audio")
        .join(name)
}

/// 16 kHz mono samples from a fixture WAV.
fn read_wav(name: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(fixture(name)).expect("fixture should exist");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000, "fixtures are 16 kHz");
    assert_eq!(spec.channels, 1, "fixtures are mono");

    reader
        .samples::<i16>()
        .map(|s| s.expect("sample") as f32 / i16::MAX as f32)
        .collect()
}

/// Download the model at most once per test binary.
///
/// Tests run in parallel; without this they all race to fetch the same file, and a test
/// that got a half-written model would "pass" by silently skipping.
static MODEL: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

fn model() -> Option<SileroVad> {
    let path = MODEL.get_or_init(|| {
        // Cached next to the build output so repeated runs do not re-download.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models");
        match ModelStore::new(dir).ensure("silero-vad-16k", |_| {}) {
            Ok(path) => Some(path),
            Err(e) => {
                eprintln!("skipping: could not obtain the VAD model ({e})");
                None
            }
        }
    });

    path.as_ref()
        .map(|path| SileroVad::load(path).expect("the model should load"))
}

/// Run a whole file through the model and segmenter.
fn segments_of(vad: &mut SileroVad, samples: &[f32], config: VadConfig) -> Vec<(usize, u32)> {
    let mut segmenter = Segmenter::new(config);
    let mut found = Vec::new();

    for frame in samples.chunks_exact(FRAME_SAMPLES) {
        let probability = vad.probability(frame).expect("inference should succeed");
        if let VadEvent::SegmentReady(segment) = segmenter.push(frame, probability) {
            found.push((segment.samples.len(), segment.duration_ms()));
        }
    }

    if let Some(segment) = segmenter.flush() {
        found.push((segment.samples.len(), segment.duration_ms()));
    }
    found
}

#[test]
fn the_model_reports_low_probability_on_silence_and_high_on_speech() {
    let Some(mut vad) = model() else { return };

    let silence = read_wav("silence.wav");
    let mut silence_max = 0.0f32;
    for frame in silence.chunks_exact(FRAME_SAMPLES) {
        silence_max = silence_max.max(vad.probability(frame).expect("inference"));
    }
    assert!(
        silence_max < 0.3,
        "silence should not look like speech, got a peak of {silence_max}"
    );

    vad.reset();

    let speech = read_wav("en_open_door.wav");
    let mut speech_max = 0.0f32;
    for frame in speech.chunks_exact(FRAME_SAMPLES) {
        speech_max = speech_max.max(vad.probability(frame).expect("inference"));
    }
    assert!(
        speech_max > 0.8,
        "speech should be detected confidently, got a peak of {speech_max}"
    );
}

#[test]
fn ukrainian_speech_is_detected_as_readily_as_english() {
    let Some(mut vad) = model() else { return };

    for fixture in ["uk_open_door.wav", "uk_sword.wav"] {
        vad.reset();
        let samples = read_wav(fixture);
        let peak = samples
            .chunks_exact(FRAME_SAMPLES)
            .map(|frame| vad.probability(frame).expect("inference"))
            .fold(0.0f32, f32::max);

        assert!(peak > 0.8, "{fixture} peaked at only {peak}");
    }
}

#[test]
fn a_single_spoken_sentence_produces_exactly_one_segment() {
    let Some(mut vad) = model() else { return };

    let samples = read_wav("en_open_door.wav");
    let segments = segments_of(&mut vad, &samples, VadConfig::default());

    assert_eq!(segments.len(), 1, "expected one segment, got {segments:?}");
    let (_, duration_ms) = segments[0];
    assert!(
        (700..=4_000).contains(&duration_ms),
        "segment duration {duration_ms} ms is implausible for one sentence"
    );
}

#[test]
fn two_sentences_with_a_long_gap_become_two_segments() {
    let Some(mut vad) = model() else { return };

    let samples = read_wav("two_sentences.wav");
    let segments = segments_of(&mut vad, &samples, VadConfig::default());

    assert_eq!(
        segments.len(),
        2,
        "a 1.5 s gap should split the sentences, got {segments:?}"
    );
}

#[test]
fn silence_alone_produces_no_segments_and_so_costs_no_transcription() {
    let Some(mut vad) = model() else { return };

    let samples = read_wav("silence.wav");
    let segments = segments_of(&mut vad, &samples, VadConfig::default());

    assert!(
        segments.is_empty(),
        "silence must never reach speech recognition, got {segments:?}"
    );
}

#[test]
fn the_segment_contains_essentially_all_of_the_speech() {
    let Some(mut vad) = model() else { return };

    let samples = read_wav("uk_open_door.wav");
    // The fixture is 0.4 s of silence, the sentence, then 0.4 s of silence.
    let speech_samples = samples.len() - (16_000 * 8 / 10);

    let segments = segments_of(&mut vad, &samples, VadConfig::default());
    assert_eq!(segments.len(), 1);

    let (captured, _) = segments[0];
    assert!(
        captured as f32 >= speech_samples as f32 * 0.9,
        "captured {captured} samples but the speech is about {speech_samples}; \
         the segmenter is clipping the sentence"
    );
}

#[test]
fn a_higher_threshold_yields_fewer_or_equal_segments() {
    let Some(mut vad) = model() else { return };
    let samples = read_wav("two_sentences.wav");

    let permissive = segments_of(
        &mut vad,
        &samples,
        VadConfig {
            speech_threshold: 0.3,
            ..VadConfig::default()
        },
    );
    vad.reset();
    let strict = segments_of(
        &mut vad,
        &samples,
        VadConfig {
            speech_threshold: 0.9,
            ..VadConfig::default()
        },
    );

    assert!(
        strict.len() <= permissive.len(),
        "a stricter threshold should not find more speech: {strict:?} vs {permissive:?}"
    );
}

#[test]
fn a_frame_of_the_wrong_length_is_rejected_rather_than_producing_nonsense() {
    let Some(mut vad) = model() else { return };
    assert!(vad.probability(&[0.0; 256]).is_err());
    assert!(vad.probability(&[]).is_err());
}
