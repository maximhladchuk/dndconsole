//! Local semantic similarity: the layer that catches phrasings nobody wrote down.
//!
//! "You notice something tiny dart between your legs" contains no keyword any event
//! lists, and no phrase any event stores. It should still reach
//! `SMALL_CREATURE_SCURRY`. That is this crate's whole job, and it does it with a
//! 113 MB embedding model on the local disk rather than an API call.

mod embed;
mod index;

pub use embed::{cosine, Embedder, EmbedderConfig, Role, EMBEDDING_DIM};
pub use index::{EmbeddingCache, NoCache, SemanticEventIndex};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not load the embedding model: {0}")]
    Model(String),

    #[error("could not load the tokenizer: {0}")]
    Tokenizer(String),

    #[error("embedding failed: {0}")]
    Embed(String),
}
