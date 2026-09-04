//! Playback commands.
//!
//! These exist so the sound engine can be exercised directly — the temporary soundboard
//! in the UI, previewing a file in the library, and manual triggers during a session.
//! Automatic, speech-driven triggering does not go through here: it goes through the
//! detection pipeline, which calls the same audio state internally.

use dndsound_sound::Bus;
use dndsound_store::sounds::Sound;
use tauri::State;

use crate::audio::PlaybackSnapshot;
use crate::commands::app::Res;
use crate::error::CommandError;
use crate::state::AppState;

#[tauri::command]
pub fn preview_sound(state: State<'_, AppState>, id: i64) -> Res<Sound> {
    let sound = state.with_db(|db| Ok::<_, CommandError>(db.sounds().get(id)?))?;
    state.audio().preview(&sound)?;
    Ok(sound)
}

/// Play one sound from a group, chosen by the group's own selection rules.
///
/// Returns the sound that was picked, or `None` when the group has nothing playable —
/// an empty group is a configuration problem to show in the UI, not an error.
#[tauri::command]
pub fn play_sound_group(state: State<'_, AppState>, id: i64) -> Res<Option<Sound>> {
    let (group, members) = state.with_db(|db| {
        let group = db.sounds().group(id)?;
        let members = db.sounds().group_members(id)?;
        Ok::<_, CommandError>((group, members))
    })?;

    let played = state.audio().play_group(&group, &members, Bus::Sfx)?;

    match &played {
        Some(sound) => tracing::debug!(group = %group.name, sound = %sound.display_name, "played"),
        None => tracing::warn!(group = %group.name, "group has no playable sounds"),
    }
    Ok(played)
}

/// Start a looping ambience bed from a single sound.
#[tauri::command]
pub fn start_ambience(state: State<'_, AppState>, id: i64) -> Res<Sound> {
    let sound = state.with_db(|db| Ok::<_, CommandError>(db.sounds().get(id)?))?;
    state.audio().start_ambience(&ambience_key(id), &sound)?;
    Ok(sound)
}

#[tauri::command]
pub fn stop_ambience(state: State<'_, AppState>, id: i64) -> Res<()> {
    state.audio().stop_ambience(&ambience_key(id))
}

#[tauri::command]
pub fn stop_all_sounds(state: State<'_, AppState>) -> Res<()> {
    state.audio().stop_all()
}

#[tauri::command]
pub fn playback_status(state: State<'_, AppState>) -> Res<PlaybackSnapshot> {
    Ok(state.audio().snapshot())
}

/// Ambience beds are keyed by sound id, so starting the same bed twice crossfades
/// instead of stacking two copies of the rain.
fn ambience_key(sound_id: i64) -> String {
    format!("sound:{sound_id}")
}
