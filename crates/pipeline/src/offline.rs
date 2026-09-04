//! Running a recorded file through the real pipeline.
//!
//! Same voice activity detection, same speech recognition, same everything — only the
//! microphone is replaced by a file. Two uses:
//!
//! * regression testing against real Dungeon Master recordings, which is the only honest
//!   way to tune detection thresholds;
//! * reproducing a session that went wrong, without asking anyone to re-perform it.

use std::path::Path;

use serde::Serialize;

use crate::stt::{SpeechRecognizer, SttConfig};
use crate::vad::{Segmenter, SileroVad, VadConfig, VadEvent, FRAME_SAMPLES};
use crate::{Error, Result, TARGET_SAMPLE_RATE};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineSegment {
    pub text: String,
    /// Where the speech starts in the file.
    pub start_ms: u32,
    pub duration_ms: u32,
    pub stt_ms: u32,
    pub language: Option<String>,
    /// False when the transcript was rejected as silence, noise or a hallucination.
    pub trusted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineRun {
    pub segments: Vec<OfflineSegment>,
    pub audio_ms: u32,
    /// Wall-clock time the whole run took, for comparison against the audio length.
    pub elapsed_ms: u32,
}

/// Decode a 16-bit PCM WAV into the mono 16 kHz the pipeline expects.
///
/// Deliberately strict about the format: silently resampling here would hide a mismatch
/// that matters, and the recorded-audio mode exists precisely to reproduce reality.
pub fn read_wav_16k_mono(path: impl AsRef<Path>) -> Result<Vec<f32>> {
    let path = path.as_ref();
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| Error::Stt(format!("could not open {}: {e}", path.display())))?;

    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(Error::Stt(format!(
            "{} has {} channels; the pipeline works in mono",
            path.display(),
            spec.channels
        )));
    }
    if spec.sample_rate != TARGET_SAMPLE_RATE {
        return Err(Error::Stt(format!(
            "{} is {} Hz; the pipeline works at {} Hz",
            path.display(),
            spec.sample_rate,
            TARGET_SAMPLE_RATE
        )));
    }

    let samples: std::result::Result<Vec<f32>, _> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|sample| sample.map(|value| value as f32 / i16::MAX as f32))
            .collect(),
        hound::SampleFormat::Float => reader.samples::<f32>().collect(),
    };

    samples.map_err(|e| Error::Stt(format!("could not read {}: {e}", path.display())))
}

/// Run samples through voice activity detection and speech recognition.
pub fn run_samples(
    samples: &[f32],
    vad_model: impl AsRef<Path>,
    speech_model: impl AsRef<Path>,
    vad_config: VadConfig,
    stt_config: SttConfig,
) -> Result<OfflineRun> {
    let started = std::time::Instant::now();

    let mut vad = SileroVad::load(vad_model)?;
    let recognizer = SpeechRecognizer::load(speech_model, stt_config)?;
    let mut segmenter = Segmenter::new(vad_config);

    let mut collected = Vec::new();
    let mut segments = Vec::new();

    for frame in samples.chunks_exact(FRAME_SAMPLES) {
        let probability = vad.probability(frame)?;
        if let VadEvent::SegmentReady(segment) = segmenter.push(frame, probability) {
            collected.push(segment);
        }
    }
    if let Some(segment) = segmenter.flush() {
        collected.push(segment);
    }

    for segment in collected {
        let transcript = recognizer.transcribe(&segment.samples, false)?;
        segments.push(OfflineSegment {
            trusted: recognizer.is_trustworthy(&transcript),
            start_ms: (segment.start_sample as f32 / TARGET_SAMPLE_RATE as f32 * 1000.0) as u32,
            duration_ms: segment.duration_ms(),
            stt_ms: transcript.elapsed_ms,
            language: transcript.language,
            text: transcript.text,
        });
    }

    Ok(OfflineRun {
        segments,
        audio_ms: (samples.len() as f32 / TARGET_SAMPLE_RATE as f32 * 1000.0) as u32,
        elapsed_ms: started.elapsed().as_millis() as u32,
    })
}

/// Run a WAV file through the pipeline.
pub fn run_file(
    path: impl AsRef<Path>,
    vad_model: impl AsRef<Path>,
    speech_model: impl AsRef<Path>,
    vad_config: VadConfig,
    stt_config: SttConfig,
) -> Result<OfflineRun> {
    let samples = read_wav_16k_mono(path)?;
    run_samples(&samples, vad_model, speech_model, vad_config, stt_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/test-audio")
            .join(name)
    }

    #[test]
    fn a_correctly_formatted_fixture_reads() {
        let samples = read_wav_16k_mono(fixture("en_open_door.wav")).expect("read");
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn a_missing_file_is_reported_with_its_path() {
        let err = read_wav_16k_mono(fixture("nope.wav")).expect_err("should fail");
        assert!(err.to_string().contains("nope.wav"), "got {err}");
    }

    #[test]
    fn the_wrong_sample_rate_is_refused_rather_than_silently_resampled() {
        // The dev sounds are 44.1 kHz, which is exactly the mistake worth catching.
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/dev-sounds/door_wood_creak_01.wav");

        let err = read_wav_16k_mono(path).expect_err("should refuse");
        assert!(err.to_string().contains("44100"), "got {err}");
    }
}
