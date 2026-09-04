//! Freesound commands: credentials, search, and importing into the local library.

use dndsound_freesound::{SearchPage, SearchQuery, Sound as RemoteSound};
use dndsound_store::sounds::Sound;
use serde::Serialize;
use tauri::State;

use crate::commands::app::Res;
use crate::error::CommandError;
use crate::freesound::{self, API_KEY_SETTING};
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreesoundStatus {
    /// Whether a key is stored. The key itself is never sent to the frontend.
    pub configured: bool,
}

#[tauri::command]
pub fn freesound_status(state: State<'_, AppState>) -> Res<FreesoundStatus> {
    let configured = state.with_db(freesound::stored_key)?.is_some();
    Ok(FreesoundStatus { configured })
}

/// Store, replace, or clear the API key.
#[tauri::command]
pub fn set_freesound_key(state: State<'_, AppState>, key: String) -> Res<FreesoundStatus> {
    let trimmed = key.trim().to_string();

    // A Freesound token is a 40-character alphanumeric string. Checking the shape here
    // turns a typo into a clear message instead of an "unauthorized" three screens later.
    if !trimmed.is_empty() && !trimmed.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(CommandError::new(
            "invalidInput",
            "Ключ Freesound складається лише з літер і цифр. Перевір, чи не лишився зайвий пробіл.",
        ));
    }

    state.with_db(|db| {
        db.settings().set(API_KEY_SETTING, &trimmed)?;
        Ok::<_, CommandError>(())
    })?;

    Ok(FreesoundStatus {
        configured: !trimmed.is_empty(),
    })
}

#[tauri::command]
pub fn freesound_search(state: State<'_, AppState>, query: SearchQuery) -> Res<SearchPage> {
    if query.text.trim().is_empty() {
        return Err(CommandError::new("invalidInput", "Напиши, що шукати."));
    }

    let client = state.with_db(freesound::client)?;
    client
        .search(&query)
        .map_err(|e| CommandError::new("freesoundFailed", e.to_string()))
}

/// Download one search result into the library.
#[tauri::command]
pub fn freesound_import(
    state: State<'_, AppState>,
    sound: RemoteSound,
    group_id: Option<i64>,
) -> Res<Sound> {
    let client = state.with_db(freesound::client)?;
    let library_dir = state.library_dir().to_path_buf();

    state.with_db(|db| {
        let imported = freesound::import(db, &client, &library_dir, &sound)?;
        if let Some(group_id) = group_id {
            db.sounds().add_to_group(group_id, imported.id)?;
        }
        Ok(imported)
    })
}

/// Every credit line the library currently owes, ready to paste into a stream
/// description or a game's credits.
#[tauri::command]
pub fn attribution_lines(state: State<'_, AppState>) -> Res<Vec<String>> {
    state.with_db(|db| {
        let mut lines: Vec<String> = db
            .sounds()
            .list()?
            .into_iter()
            .filter(|s| s.provenance.requires_attribution())
            .map(|s| s.provenance.attribution)
            .collect();
        lines.sort();
        lines.dedup();
        Ok(lines)
    })
}
