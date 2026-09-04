//! Prints VAD probabilities and segmenter events for a fixture, for tuning by eye.
use dndsound_models::ModelStore;
use dndsound_pipeline::{Segmenter, SileroVad, VadConfig, VadEvent, FRAME_SAMPLES};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let name = std::env::args().nth(1).unwrap_or("uk_open_door.wav".into());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/test-audio")
        .join(&name);

    let mut reader = hound::WavReader::open(&path)?;
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect();

    let store = ModelStore::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-models"),
    );
    let mut vad = SileroVad::load(store.ensure("silero-vad-16k", |_| {})?)?;
    let mut segmenter = Segmenter::new(VadConfig::default());

    let mut line = String::new();
    for (i, frame) in samples.chunks_exact(FRAME_SAMPLES).enumerate() {
        let p = vad.probability(frame)?;
        line.push(match (p * 10.0) as u32 {
            0 => '.',
            1..=4 => '-',
            5..=7 => '+',
            _ => '#',
        });
        match segmenter.push(frame, p) {
            VadEvent::SpeechStarted => println!("frame {i}: SPEECH STARTED  p={p:.2}"),
            VadEvent::SegmentReady(s) => println!(
                "frame {i}: SEGMENT {} samples / {} ms (truncated={})",
                s.samples.len(),
                s.duration_ms(),
                s.truncated
            ),
            _ => {}
        }
    }
    println!("{name}: {} frames\n{line}", samples.len() / FRAME_SAMPLES);

    match segmenter.flush() {
        Some(s) => println!(
            "flush -> {} samples / {} ms",
            s.samples.len(),
            s.duration_ms()
        ),
        None => println!("flush -> nothing in progress"),
    }
    Ok(())
}
