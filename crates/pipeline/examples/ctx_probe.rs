//! Sweeps the `audio_ctx` floor to find where trimming starts destroying transcripts.
//!
//! Trimming the encoder context is the biggest speed win in the pipeline, and past a
//! point it is also the biggest accuracy loss: Ukrainian comes back misspelled and short
//! utterances collapse into a repetition loop. This finds the knee, on this machine.

use std::path::{Path, PathBuf};
use std::time::Instant;

use dndsound_models::ModelStore;
use dndsound_pipeline::{SpeechRecognizer, SttConfig};

fn read_wav(path: &Path) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).expect("wav");
    reader
        .samples::<i16>()
        .map(|s| s.expect("s") as f32 / i16::MAX as f32)
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let store =
        ModelStore::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models"));
    let turbo = store.ensure("large-v3-turbo-q5_0", |_| {})?;

    for floor in [128, 256, 384, 512, 640, 768, 1024] {
        let recognizer = SpeechRecognizer::load(
            &turbo,
            SttConfig {
                language: Some("uk".to_string()),
                audio_context_floor: floor,
                ..SttConfig::default()
            },
        )?;
        println!("\n## audio_ctx floor = {floor}");
        for file in &files {
            let samples = read_wav(file);
            // Two runs; report the second so model warm-up is not counted.
            let _ = recognizer.transcribe(&samples, false)?;
            let started = Instant::now();
            let t = recognizer.transcribe(&samples, false)?;
            let text: String = t.text.chars().take(80).collect();
            println!(
                "  {:<22} {:>5} ms  {:?}",
                file.file_name().unwrap().to_string_lossy(),
                started.elapsed().as_millis(),
                text
            );
        }
    }
    Ok(())
}
