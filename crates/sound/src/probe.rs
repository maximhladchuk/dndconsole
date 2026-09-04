//! Reading a sound file's metadata without decoding all of it.
//!
//! Import needs duration, sample rate, channel count and format for the library UI.
//! Decoding a five-minute ambience bed just to learn it is five minutes long would make
//! importing a folder painfully slow, so this reads the container headers instead.

use std::fs::File;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::{Error, Result};

/// Extensions the importer accepts. Anything else is rejected with a clear message
/// rather than being imported and failing later at play time.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "oga", "flac"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundMetadata {
    pub format: String,
    pub duration_ms: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
}

pub fn is_supported(path: impl AsRef<Path>) -> bool {
    extension_of(path.as_ref())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or(false)
}

fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
}

/// Read what the container header says about the file.
///
/// Duration is `None` for formats that do not record a frame count in the header (some
/// streamed MP3s, for instance). That is reported honestly rather than guessed at.
pub fn probe(path: impl AsRef<Path>) -> Result<SoundMetadata> {
    let path: PathBuf = path.as_ref().to_path_buf();

    if !path.is_file() {
        return Err(Error::Missing(path));
    }

    let extension = extension_of(&path).unwrap_or_default();
    let file = File::open(&path).map_err(|e| Error::Play {
        path: path.clone(),
        reason: e.to_string(),
    })?;

    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if !extension.is_empty() {
        hint.with_extension(&extension);
    }

    let reader = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| Error::Play {
            path: path.clone(),
            reason: format!("unsupported or corrupt audio file: {e}"),
        })?;

    // Pick the first track that actually carries audio parameters; a container can
    // hold video or subtitle tracks alongside the audio.
    let track = reader
        .tracks()
        .iter()
        .find(|t| matches!(t.codec_params, Some(CodecParameters::Audio(_))))
        .ok_or_else(|| Error::Play {
            path: path.clone(),
            reason: "file contains no audio tracks".to_string(),
        })?;

    let Some(CodecParameters::Audio(params)) = &track.codec_params else {
        return Err(Error::Play {
            path,
            reason: "audio track has no codec parameters".to_string(),
        });
    };

    let sample_rate = params.sample_rate;
    let duration_ms = match (track.num_frames, sample_rate) {
        (Some(frames), Some(rate)) if rate > 0 => {
            Some((frames as f64 / rate as f64 * 1000.0).round() as i64)
        }
        _ => None,
    };

    Ok(SoundMetadata {
        format: extension,
        duration_ms,
        sample_rate: sample_rate.map(|r| r as i64),
        channels: params.channels.as_ref().map(|c| c.count() as i64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/dev-sounds")
            .join(name)
    }

    #[test]
    fn extension_support_is_case_insensitive() {
        assert!(is_supported("a.wav"));
        assert!(is_supported("A.WAV"));
        assert!(is_supported("b.mp3"));
        assert!(is_supported("c.ogg"));
        assert!(is_supported("d.flac"));

        assert!(!is_supported("e.aiff"));
        assert!(!is_supported("f.txt"));
        assert!(!is_supported("no-extension"));
    }

    #[test]
    fn a_real_wav_reports_its_actual_metadata() {
        let meta = probe(asset("door_wood_creak_01.wav")).expect("probe");

        assert_eq!(meta.format, "wav");
        assert_eq!(meta.sample_rate, Some(44_100));
        assert_eq!(meta.channels, Some(1));

        // The generator writes 1.40 s; allow a frame or two of rounding.
        let duration = meta.duration_ms.expect("wav headers carry a frame count");
        assert!((1_380..=1_420).contains(&duration), "got {duration} ms");
    }

    #[test]
    fn a_longer_file_reports_a_longer_duration() {
        let short = probe(asset("sword_swing_01.wav")).expect("probe");
        let long = probe(asset("thunder_01.wav")).expect("probe");
        assert!(long.duration_ms > short.duration_ms);
    }

    #[test]
    fn a_missing_file_is_reported_as_missing() {
        let err = probe(asset("nope.wav")).expect_err("should fail");
        assert!(matches!(err, Error::Missing(_)), "got {err:?}");
    }

    #[test]
    fn a_file_that_is_not_audio_is_rejected_with_an_explanation() {
        let path = std::env::temp_dir().join("dndsound-probe-garbage.wav");
        std::fs::write(&path, b"definitely not audio").expect("write");

        let err = probe(&path).expect_err("should fail");
        assert!(
            err.to_string().contains("unsupported or corrupt"),
            "got {err}"
        );

        std::fs::remove_file(&path).ok();
    }
}
