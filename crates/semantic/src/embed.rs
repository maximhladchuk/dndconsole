//! Sentence embeddings with multilingual-e5-small.
//!
//! The graph, read from the model itself
//! (`cargo run -p dndsound-semantic --example inspect_e5`):
//!
//! ```text
//! inputs   input_ids      : i64 [batch, sequence]
//!          attention_mask : i64 [batch, sequence]
//!          token_type_ids : i64 [batch, sequence]
//! output   last_hidden_state : f32 [batch, sequence, 384]
//! ```
//!
//! Two details are easy to get wrong and fail silently rather than loudly:
//!
//! * The output is per-token hidden states, not a sentence vector. It has to be
//!   **mean-pooled over the attention mask** — pooling over padding drags every vector
//!   towards the same point and quietly destroys the similarity signal.
//! * E5 was trained with `query: ` and `passage: ` prefixes. Without them similarity
//!   still "works", just worse, which is the hardest kind of bug to notice.

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;

use crate::{Error, Result};

/// Output dimensionality of multilingual-e5-small.
pub const EMBEDDING_DIM: usize = 384;

/// Longest input in tokens. Narration segments are short; anything longer is a runaway
/// transcript, and truncating costs less than the time to embed it.
const MAX_TOKENS: usize = 128;

/// Which side of the E5 prompt convention a text is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Live narration.
    Query,
    /// A stored example phrase.
    Passage,
}

impl Role {
    fn prefix(self) -> &'static str {
        match self {
            Role::Query => "query: ",
            Role::Passage => "passage: ",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbedderConfig {
    pub threads: u16,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self { threads: 4 }
    }
}

pub struct Embedder {
    session: std::sync::Mutex<Session>,
    tokenizer: Tokenizer,
}

impl Embedder {
    pub fn load(
        model_path: impl AsRef<Path>,
        tokenizer_path: impl AsRef<Path>,
        config: EmbedderConfig,
    ) -> Result<Self> {
        let tokenizer = Tokenizer::from_file(tokenizer_path.as_ref())
            .map_err(|e| Error::Tokenizer(e.to_string()))?;

        // `with_intra_threads` consumes the builder and hands a new one back.
        let session = Session::builder()
            .map_err(|e| Error::Model(e.to_string()))?
            .with_intra_threads(config.threads as usize)
            .map_err(|e| Error::Model(e.to_string()))?
            .commit_from_file(model_path.as_ref())
            .map_err(|e| Error::Model(e.to_string()))?;

        Ok(Self {
            session: std::sync::Mutex::new(session),
            tokenizer,
        })
    }

    /// Embed one text. The vector is L2-normalized, so cosine similarity is a dot product.
    pub fn embed(&self, text: &str, role: Role) -> Result<Vec<f32>> {
        Ok(self.embed_batch(&[text], role)?.remove(0))
    }

    /// Embed several texts at once. Batching matters when indexing hundreds of phrases.
    pub fn embed_batch(&self, texts: &[&str], role: Role) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let prefixed: Vec<String> = texts
            .iter()
            .map(|text| format!("{}{}", role.prefix(), text))
            .collect();

        let encodings = self
            .tokenizer
            .encode_batch(
                prefixed.iter().map(String::as_str).collect::<Vec<_>>(),
                true,
            )
            .map_err(|e| Error::Embed(e.to_string()))?;

        let batch = encodings.len();
        let length = encodings
            .iter()
            .map(|encoding| encoding.get_ids().len().min(MAX_TOKENS))
            .max()
            .unwrap_or(1)
            .max(1);

        let mut input_ids = vec![0i64; batch * length];
        let mut attention_mask = vec![0i64; batch * length];
        let token_type_ids = vec![0i64; batch * length];

        for (row, encoding) in encodings.iter().enumerate() {
            let ids = encoding.get_ids();
            let mask = encoding.get_attention_mask();
            let take = ids.len().min(length);

            for column in 0..take {
                input_ids[row * length + column] = i64::from(ids[column]);
                attention_mask[row * length + column] = i64::from(mask[column]);
            }
        }

        let shape = [batch, length];
        let outputs = {
            let mut session = self
                .session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            let result = session
                .run(ort::inputs! {
                    "input_ids" => Tensor::from_array((shape, input_ids)).map_err(embed_error)?,
                    "attention_mask" => Tensor::from_array((shape, attention_mask.clone()))
                        .map_err(embed_error)?,
                    "token_type_ids" => Tensor::from_array((shape, token_type_ids))
                        .map_err(embed_error)?,
                })
                .map_err(embed_error)?;

            let (_, data) = result["last_hidden_state"]
                .try_extract_tensor::<f32>()
                .map_err(embed_error)?;
            data.to_vec()
        };

        Ok(mean_pool(&outputs, &attention_mask, batch, length))
    }
}

fn embed_error(err: impl std::fmt::Display) -> Error {
    Error::Embed(err.to_string())
}

/// Average the token vectors that are not padding, then normalize to unit length.
fn mean_pool(hidden: &[f32], attention_mask: &[i64], batch: usize, length: usize) -> Vec<Vec<f32>> {
    let mut result = Vec::with_capacity(batch);

    for row in 0..batch {
        let mut summed = vec![0.0f32; EMBEDDING_DIM];
        let mut counted = 0.0f32;

        for token in 0..length {
            if attention_mask[row * length + token] == 0 {
                continue;
            }
            counted += 1.0;
            let offset = (row * length + token) * EMBEDDING_DIM;
            for dimension in 0..EMBEDDING_DIM {
                summed[dimension] += hidden[offset + dimension];
            }
        }

        if counted > 0.0 {
            for value in &mut summed {
                *value /= counted;
            }
        }

        normalize(&mut summed);
        result.push(summed);
    }

    result
}

/// Scale to unit length so cosine similarity is a plain dot product.
fn normalize(vector: &mut [f32]) {
    let magnitude = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if magnitude > f32::EPSILON {
        for value in vector {
            *value /= magnitude;
        }
    }
}

/// Cosine similarity of two normalized vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pooling_ignores_padding() {
        // Two tokens of content, two of padding, in a batch of one.
        let length = 4;
        let mut hidden = vec![0.0f32; length * EMBEDDING_DIM];
        // Real tokens point along dimension 0; padding points the opposite way, so
        // including it would visibly change the result.
        hidden[0] = 1.0;
        hidden[EMBEDDING_DIM] = 1.0;
        hidden[2 * EMBEDDING_DIM] = -50.0;
        hidden[3 * EMBEDDING_DIM] = -50.0;

        let mask = vec![1, 1, 0, 0];
        let pooled = mean_pool(&hidden, &mask, 1, length);

        assert_eq!(pooled.len(), 1);
        assert!(
            (pooled[0][0] - 1.0).abs() < 1e-5,
            "padding leaked into the pooled vector: {}",
            pooled[0][0]
        );
    }

    #[test]
    fn pooled_vectors_are_unit_length() {
        let length = 2;
        let mut hidden = vec![0.0f32; length * EMBEDDING_DIM];
        hidden[0] = 3.0;
        hidden[1] = 4.0;
        hidden[EMBEDDING_DIM] = 3.0;
        hidden[EMBEDDING_DIM + 1] = 4.0;

        let pooled = mean_pool(&hidden, &[1, 1], 1, length);
        let magnitude = pooled[0].iter().map(|v| v * v).sum::<f32>().sqrt();

        assert!((magnitude - 1.0).abs() < 1e-5, "magnitude was {magnitude}");
    }

    #[test]
    fn an_all_padding_row_does_not_divide_by_zero() {
        let pooled = mean_pool(&vec![0.0; EMBEDDING_DIM], &[0], 1, 1);
        assert!(pooled[0].iter().all(|v| v.is_finite()));
    }

    #[test]
    fn cosine_of_identical_vectors_is_one() {
        let mut a = vec![0.0f32; EMBEDDING_DIM];
        a[0] = 1.0;
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_orthogonal_vectors_is_zero() {
        let mut a = vec![0.0f32; EMBEDDING_DIM];
        let mut b = vec![0.0f32; EMBEDDING_DIM];
        a[0] = 1.0;
        b[1] = 1.0;
        assert!(cosine(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_mismatched_lengths_is_zero_rather_than_a_panic() {
        assert_eq!(cosine(&[1.0, 0.0], &[1.0]), 0.0);
    }

    #[test]
    fn the_prompt_prefixes_match_the_e5_convention() {
        assert_eq!(Role::Query.prefix(), "query: ");
        assert_eq!(Role::Passage.prefix(), "passage: ");
    }
}
