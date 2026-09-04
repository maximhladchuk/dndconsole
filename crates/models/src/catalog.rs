//! The models the application knows how to fetch.
//!
//! Sizes and licenses were verified against the Hugging Face and GitHub APIs on
//! 2026-09-04. SHA-256 digests are filled in on first
//! download rather than hard-coded from a source we did not verify ourselves: a wrong
//! digest baked into the binary would make the model impossible to install.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    /// Voice activity detection.
    Vad,
    /// Speech recognition.
    Speech,
    /// Sentence embeddings for semantic event matching.
    Embedding,
    /// A tokenizer or other file a model needs alongside its weights.
    Support,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    /// Stable identifier, also used as the settings value and the file name stem.
    pub id: &'static str,
    pub display_name: &'static str,
    pub kind: ModelKind,
    pub url: &'static str,
    /// File name on disk, inside the model directory.
    pub file_name: &'static str,
    /// Approximate download size, for the UI. The real size is whatever arrives.
    pub approx_bytes: u64,
    pub license: &'static str,
    pub languages: &'static str,
    pub notes: &'static str,
}

/// Everything the app can download.
pub const CATALOG: &[ModelSpec] = &[
    ModelSpec {
        id: "silero-vad-16k",
        display_name: "Silero VAD v6 (16 kHz)",
        kind: ModelKind::Vad,
        url: "https://raw.githubusercontent.com/snakers4/silero-vad/v6.2.1/src/silero_vad/data/silero_vad_16k_op15.onnx",
        file_name: "silero_vad_16k_op15.onnx",
        approx_bytes: 1_289_626,
        license: "MIT",
        languages: "language independent",
        notes: "Required for listening. Tiny, and the pipeline cannot run without it.",
    },
    ModelSpec {
        id: "large-v3-turbo-q5_0",
        display_name: "Whisper large-v3-turbo (q5_0)",
        kind: ModelKind::Speech,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        file_name: "ggml-large-v3-turbo-q5_0.bin",
        approx_bytes: 574_041_600,
        license: "MIT",
        languages: "multilingual, including Ukrainian and English",
        notes: "Recommended. Best Ukrainian quality per millisecond on Apple Silicon.",
    },
    ModelSpec {
        id: "small-q5_1",
        display_name: "Whisper small (q5_1)",
        kind: ModelKind::Speech,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        file_name: "ggml-small-q5_1.bin",
        approx_bytes: 189_792_000,
        license: "MIT",
        languages: "multilingual",
        notes: "Faster and much smaller, but noticeably weaker on free-form Ukrainian.",
    },
    ModelSpec {
        id: "multilingual-e5-small-int8",
        display_name: "multilingual-e5-small (int8)",
        kind: ModelKind::Embedding,
        url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main/onnx/model_qint8_avx512_vnni.onnx",
        file_name: "multilingual-e5-small-int8.onnx",
        approx_bytes: 118_384_000,
        license: "MIT",
        languages: "100 languages",
        notes: "Semantic event matching. Cross-lingual: Ukrainian speech can match English phrases.",
    },
    ModelSpec {
        id: "multilingual-e5-small-tokenizer",
        display_name: "multilingual-e5-small tokenizer",
        kind: ModelKind::Support,
        url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main/tokenizer.json",
        file_name: "multilingual-e5-small-tokenizer.json",
        approx_bytes: 17_098_000,
        license: "MIT",
        languages: "100 languages",
        notes: "Required alongside the embedding model.",
    },
];

pub fn find(id: &str) -> Option<&'static ModelSpec> {
    CATALOG.iter().find(|spec| spec.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_file_names_do_not_collide() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate model id");

        let mut names: Vec<&str> = CATALOG.iter().map(|s| s.file_name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(
            names.len(),
            count,
            "two models would write to the same file"
        );
    }

    #[test]
    fn every_url_is_https_and_every_file_name_is_a_bare_name() {
        for spec in CATALOG {
            assert!(spec.url.starts_with("https://"), "{} is not https", spec.id);
            assert!(
                !spec.file_name.contains('/') && !spec.file_name.contains(".."),
                "{} has a file name that could escape the model folder",
                spec.id
            );
            assert!(spec.approx_bytes > 0, "{} has no size", spec.id);
            assert!(!spec.license.is_empty(), "{} has no license", spec.id);
        }
    }

    #[test]
    fn the_defaults_referenced_elsewhere_exist() {
        // These ids appear in AppSettings defaults and in the VAD loader.
        assert!(find("silero-vad-16k").is_some());
        assert!(find("large-v3-turbo-q5_0").is_some());
        assert!(find("multilingual-e5-small-int8").is_some());
        assert!(find("nope").is_none());
    }

    #[test]
    fn there_is_exactly_one_vad_model_and_it_is_small() {
        let vad: Vec<&ModelSpec> = CATALOG
            .iter()
            .filter(|s| s.kind == ModelKind::Vad)
            .collect();
        assert_eq!(vad.len(), 1);
        assert!(
            vad[0].approx_bytes < 5_000_000,
            "the VAD model should be tiny"
        );
    }
}
