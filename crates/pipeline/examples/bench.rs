//! Measures the pipeline stages on this machine.
//!
//! Every number in `docs/PERFORMANCE.md` comes from this program. Run with:
//!
//! ```text
//! cargo run --release -p dndsound-pipeline --example bench
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use dndsound_models::ModelStore;
use dndsound_pipeline::{
    MonoResampler, SileroVad, SpeechRecognizer, SttConfig, FRAME_SAMPLES, TARGET_SAMPLE_RATE,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/test-audio")
        .join(name)
}

fn read_wav(name: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(fixture(name)).expect("fixture");
    reader
        .samples::<i16>()
        .map(|s| s.expect("sample") as f32 / i16::MAX as f32)
        .collect()
}

/// Median is reported rather than mean: one scheduling hiccup should not define the
/// number people plan around.
fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store =
        ModelStore::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models"));

    println!("# Measured on this machine, {}\n", chrono_stamp());

    // --- VAD ---------------------------------------------------------------
    let vad_path = store.ensure("silero-vad-16k", |_| {})?;
    let load_started = Instant::now();
    let mut vad = SileroVad::load(&vad_path)?;
    println!("VAD model load: {} ms", load_started.elapsed().as_millis());

    let speech = read_wav("en_open_door.wav");
    let mut frame_times = Vec::new();
    for frame in speech.chunks_exact(FRAME_SAMPLES) {
        let started = Instant::now();
        vad.probability(frame)?;
        frame_times.push(started.elapsed().as_micros());
    }
    println!(
        "VAD per 512-sample frame (32 ms of audio): {} µs median over {} frames",
        median(frame_times.clone()),
        frame_times.len()
    );

    // --- resampling --------------------------------------------------------
    let mut resampler = MonoResampler::new(48_000)?;
    let one_second: Vec<f32> = (0..48_000)
        .map(|i| (i as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU).sin())
        .collect();
    let mut out = Vec::new();
    let started = Instant::now();
    resampler.push(&one_second, &mut out)?;
    println!(
        "Resample 1 s of 48 kHz to 16 kHz: {} µs",
        started.elapsed().as_micros()
    );

    // --- STT ---------------------------------------------------------------
    let model_path = match store.ensure("large-v3-turbo-q5_0", |_| {}) {
        Ok(path) => path,
        Err(e) => {
            println!("\n(skipping speech recognition: {e})");
            return Ok(());
        }
    };

    let load_started = Instant::now();
    let recognizer = SpeechRecognizer::load(&model_path, SttConfig::default())?;
    println!(
        "\nWhisper large-v3-turbo-q5_0 load: {} ms",
        load_started.elapsed().as_millis()
    );

    let untrimmed = SpeechRecognizer::load(
        &model_path,
        SttConfig {
            trim_audio_context: false,
            ..SttConfig::default()
        },
    )?;

    for (label, name) in [
        ("English, 3.0 s", "en_open_door.wav"),
        ("Ukrainian, 3.9 s", "uk_open_door.wav"),
        ("Ukrainian, 3.6 s", "uk_sword.wav"),
    ] {
        let samples = read_wav(name);
        let seconds = samples.len() as f32 / TARGET_SAMPLE_RATE as f32;

        // Warm up, then take the median of five runs.
        let _ = recognizer.transcribe(&samples, false)?;
        let trimmed_times: Vec<u128> = (0..5)
            .map(|_| {
                let started = Instant::now();
                let _ = recognizer.transcribe(&samples, false);
                started.elapsed().as_millis()
            })
            .collect();

        let _ = untrimmed.transcribe(&samples, false)?;
        let untrimmed_times: Vec<u128> = (0..3)
            .map(|_| {
                let started = Instant::now();
                let _ = untrimmed.transcribe(&samples, false);
                started.elapsed().as_millis()
            })
            .collect();

        let trimmed_ms = median(trimmed_times);
        println!(
            "STT {label} ({seconds:.1} s audio): {trimmed_ms} ms trimmed, {} ms untrimmed \
             (real-time factor {:.2}x)",
            median(untrimmed_times),
            seconds * 1000.0 / trimmed_ms as f32
        );
    }

    // The small model is the candidate for partial re-decodes if turbo is too slow.
    if let Ok(small_path) = store.ensure("small-q5_1", |_| {}) {
        let small = SpeechRecognizer::load(&small_path, SttConfig::default())?;
        for (label, name) in [
            ("English, 3.0 s", "en_open_door.wav"),
            ("Ukrainian, 3.9 s", "uk_open_door.wav"),
        ] {
            let samples = read_wav(name);
            let _ = small.transcribe(&samples, false)?;
            let times: Vec<u128> = (0..5)
                .map(|_| {
                    let started = Instant::now();
                    let _ = small.transcribe(&samples, false);
                    started.elapsed().as_millis()
                })
                .collect();
            let transcript = small.transcribe(&samples, false)?;
            println!(
                "STT small-q5_1 {label}: {} ms -> {:?}",
                median(times),
                transcript.text
            );
        }
    }

    // Short clips matter most: they are what a partial re-decode looks like.
    let full = read_wav("en_open_door.wav");
    for seconds in [1.0_f32, 2.0] {
        let count = (seconds * TARGET_SAMPLE_RATE as f32) as usize;
        let clip = &full[..count.min(full.len())];

        let _ = recognizer.transcribe(clip, true)?;
        let times: Vec<u128> = (0..5)
            .map(|_| {
                let started = Instant::now();
                let _ = recognizer.transcribe(clip, true);
                started.elapsed().as_millis()
            })
            .collect();
        println!(
            "STT partial re-decode of {seconds:.0} s: {} ms",
            median(times)
        );
    }

    Ok(())
}

/// The date, without pulling in a date library for one line.
fn chrono_stamp() -> String {
    let output = std::process::Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok();
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
