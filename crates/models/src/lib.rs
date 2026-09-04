//! Local model management: what models exist, where they live, and getting them here.
//!
//! The product rule is that runtime works offline. Downloading is therefore a one-time
//! setup step, never something the app does during a session. Once a model is on disk
//! and verified, nothing here touches the network again.

mod catalog;
mod download;
mod error;

pub use catalog::{ModelKind, ModelSpec, CATALOG};
pub use download::{DownloadProgress, ModelStore};
pub use error::{Error, Result};
