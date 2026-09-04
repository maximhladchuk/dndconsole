//! Sound pack commands: check, install, and report what is there.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::app::Res;
use crate::sound_pack::{self, InstallReport};
use crate::state::AppState;

/// Progress is emitted on this channel so a first-launch install can show a bar rather
/// than a frozen window.
const PROGRESS_EVENT: &str = "pack://progress";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackStatus {
    /// Every bundled sound is present and playable.
    pub installed: bool,
    /// How many sounds the pack contains.
    pub total: usize,
    /// How many are already in the library.
    pub present: usize,
    /// Rough download size, for the first-launch prompt.
    pub megabytes: f32,
}

#[tauri::command]
pub fn sound_pack_status(state: State<'_, AppState>) -> Res<PackStatus> {
    let manifest = dndsound_pack::Manifest::bundled();
    let present =
        state.with_db(|db| Ok::<_, crate::error::CommandError>(db.sounds().count()? as usize))?;

    Ok(PackStatus {
        installed: present >= manifest.sounds.len(),
        total: manifest.sounds.len(),
        present,
        megabytes: manifest.total_bytes_estimate() as f32 / 1_048_576.0,
    })
}

/// Download whatever is missing. Safe to run repeatedly: cached files are reused.
#[tauri::command]
pub fn install_sound_pack(app: AppHandle, state: State<'_, AppState>) -> Res<InstallReport> {
    let cache_dir = state.library_dir().join("pack");
    state.with_db(|db| {
        sound_pack::install(db, &cache_dir, |progress| {
            let _ = app.emit(PROGRESS_EVENT, progress);
        })
    })
}
