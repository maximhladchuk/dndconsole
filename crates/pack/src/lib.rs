//! The sound pack: which sounds the application ships with, and where they come from.
//!
//! Two layers. `catalog` is the curated list — event id, group name, Freesound ids —
//! maintained by hand. `manifest` is that list resolved against Freesound into download
//! URLs, committed alongside so that installing the pack needs no API key.
//!
//! This crate is pure data. Downloading lives in the application, next to the database
//! it writes into.

pub mod catalog;
pub mod manifest;

pub use catalog::{theme_for, Theme, THEMES};
pub use manifest::{Manifest, ManifestSound};
