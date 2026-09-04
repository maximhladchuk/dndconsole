//! Measures what automatic language detection costs and whether restricting it helps.
//!
//! ```text
//! cargo run --release -p dndsound-pipeline --example lang_probe -- <wav>...
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use dndsound_models::ModelStore;
use dndsound_pipeline::{SpeechRecognizer, SttConfig};

fn read_wav(path: &Path) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).expect("wav");
    reader
        .samples::<i16>()
        .map(|s| s.expect("sample") as f32 / i16::MAX as f32)
        .collect()
}

fn run(label: &str, model: &Path, config: SttConfig, files: &[PathBuf]) {
    let recognizer = SpeechRecognizer::load(model, config).expect("load");
    println!("\n## {label}");
    for file in files {
        let samples = read_wav(file);
        let started = Instant::now();
        let transcript = recognizer.transcribe(&samples, false).expect("transcribe");
        println!(
            "  {:<22} {:>5} ms  lang={:<4} {:?}",
            file.file_name().unwrap().to_string_lossy(),
            started.elapsed().as_millis(),
            transcript.language.as_deref().unwrap_or("?"),
            transcript.text
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    assert!(!files.is_empty(), "pass at least one wav");

    let store =
        ModelStore::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models"));
    let turbo = store.ensure("large-v3-turbo-q5_0", |_| {})?;
    let small = store.ensure("small-q5_1", |_| {})?;

    let unrestricted = SttConfig {
        auto_languages: Vec::new(),
        ..SttConfig::default()
    };

    run(
        "turbo · whisper's own 99-way detection",
        &turbo,
        unrestricted.clone(),
        &files,
    );
    run(
        "turbo · detection restricted to uk/en",
        &turbo,
        SttConfig::default(),
        &files,
    );
    run(
        "turbo · language pinned to uk (no detection pass)",
        &turbo,
        SttConfig {
            language: Some("uk".to_string()),
            ..SttConfig::default()
        },
        &files,
    );
    run(
        "turbo · language pinned to en (no detection pass)",
        &turbo,
        SttConfig {
            language: Some("en".to_string()),
            ..SttConfig::default()
        },
        &files,
    );
    let _ = (small, unrestricted);

    Ok(())
}
