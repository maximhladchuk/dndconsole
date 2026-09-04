//! Model management: what is available, what is downloaded, and fetching it.
//!
//! Downloading is the one thing the application does over the network, and only when the
//! user asks. Once a model is on disk, the app never touches the network again.

use dndsound_models::{ModelKind, ModelSpec, CATALOG};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::app::Res;
use crate::error::CommandError;
use crate::state::AppState;

/// Tauri event carrying download progress.
pub const DOWNLOAD_EVENT: &str = "models://progress";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub kind: ModelKind,
    pub approx_bytes: u64,
    pub license: String,
    pub languages: String,
    pub notes: String,
    pub downloaded: bool,
    pub size_on_disk: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadUpdate {
    pub id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub done: bool,
}

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> Res<Vec<ModelInfo>> {
    Ok(CATALOG.iter().map(|spec| describe(&state, spec)).collect())
}

/// Download a model, reporting progress as it goes.
///
/// Progress is throttled to roughly one event per megabyte: a 547 MB model would
/// otherwise emit thousands of IPC messages for a progress bar that only needs to move.
///
/// `async` so the download runs off the main thread; see `install_sound_pack` for what
/// a long command does to the window without it.
#[tauri::command(async)]
pub fn download_model(app: AppHandle, state: State<'_, AppState>, id: String) -> Res<ModelInfo> {
    let mut last_reported = 0u64;

    state
        .models()
        .ensure(&id, |progress| {
            if progress.downloaded_bytes - last_reported >= 1_048_576 {
                last_reported = progress.downloaded_bytes;
                let _ = app.emit(
                    DOWNLOAD_EVENT,
                    DownloadUpdate {
                        id: id.clone(),
                        downloaded_bytes: progress.downloaded_bytes,
                        total_bytes: progress.total_bytes,
                        done: false,
                    },
                );
            }
        })
        .map_err(model_error)?;

    let _ = app.emit(
        DOWNLOAD_EVENT,
        DownloadUpdate {
            id: id.clone(),
            downloaded_bytes: 0,
            total_bytes: None,
            done: true,
        },
    );

    // A freshly downloaded embedding model should take effect immediately.
    if id == crate::semantic::EMBEDDING_MODEL_ID || id == crate::semantic::EMBEDDING_TOKENIZER_ID {
        let db = state.db_handle();
        state.detection().attach_semantic(state.models(), &db);
    }

    let spec = dndsound_models::CATALOG
        .iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| CommandError::new("unknownModel", format!("Невідома модель «{id}».")))?;

    Ok(describe(&state, spec))
}

/// Recompute a downloaded model's checksum and compare it with the one recorded when it
/// was fetched. Hashing half a gigabyte is not a main-thread job either.
#[tauri::command(async)]
pub fn verify_model(state: State<'_, AppState>, id: String) -> Res<String> {
    state.models().verify(&id).map_err(model_error)
}

#[tauri::command]
pub fn delete_model(state: State<'_, AppState>, id: String) -> Res<Vec<ModelInfo>> {
    if state.session().is_running() {
        return Err(CommandError::new(
            "sessionRunning",
            "Зупини сесію прослуховування, перш ніж видаляти модель.",
        ));
    }

    state.models().remove(&id).map_err(model_error)?;
    list_models(state)
}

#[tauri::command]
pub fn model_directory(state: State<'_, AppState>) -> Res<String> {
    Ok(state.models().dir().display().to_string())
}

fn describe(state: &State<'_, AppState>, spec: &ModelSpec) -> ModelInfo {
    ModelInfo {
        id: spec.id.to_string(),
        display_name: spec.display_name.to_string(),
        kind: spec.kind,
        approx_bytes: spec.approx_bytes,
        license: spec.license.to_string(),
        languages: spec.languages.to_string(),
        notes: spec.notes.to_string(),
        downloaded: state.models().is_downloaded(spec),
        size_on_disk: state.models().size_on_disk(spec),
    }
}

fn model_error(err: dndsound_models::Error) -> CommandError {
    use dndsound_models::Error as E;
    let kind = match err {
        E::UnknownModel(_) => "unknownModel",
        E::NotDownloaded { .. } => "modelMissing",
        E::Download { .. } => "downloadFailed",
        E::ChecksumMismatch { .. } => "modelCorrupt",
        E::Io { .. } => "io",
    };
    tracing::error!(error = %err, "model error");
    CommandError::new(kind, err.to_string())
}
