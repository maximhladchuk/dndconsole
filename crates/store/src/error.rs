use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("database migration failed: {0}")]
    Migration(String),

    #[error("could not read or write the database file: {0}")]
    Io(#[from] std::io::Error),

    #[error("stored value for '{key}' could not be decoded: {source}")]
    Decode {
        key: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("value could not be encoded for storage: {0}")]
    Encode(#[source] serde_json::Error),

    #[error("{0} not found")]
    NotFound(String),
}
