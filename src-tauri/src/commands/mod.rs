//! Tauri commands — the only surface the frontend can call.
//!
//! Commands validate their input, delegate to a service or repository, and convert
//! errors into something the UI can explain. They hold no business logic.

pub mod app;
pub mod events;
pub mod freesound;
pub mod library;
pub mod microphone;
pub mod models;
pub mod playback;
pub mod session;
pub mod sound_pack;
