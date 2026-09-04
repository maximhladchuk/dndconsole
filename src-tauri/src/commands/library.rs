//! Sound library commands: importing files, editing metadata, and sound groups.

use std::path::PathBuf;

use dndsound_store::sounds::{Sound, SoundGroup};
use tauri::State;

use crate::commands::app::Res;
use crate::error::CommandError;
use crate::library::{self, ImportOptions, ImportReport};
use crate::state::AppState;

fn options(state: &AppState) -> Res<ImportOptions> {
    let managed =
        state.with_db(|db| Ok::<_, CommandError>(db.settings().load()?.managed_library))?;
    Ok(ImportOptions {
        managed,
        library_dir: state.library_dir().to_path_buf(),
    })
}

#[tauri::command]
pub fn list_sounds(state: State<'_, AppState>) -> Res<Vec<Sound>> {
    state.with_db(|db| Ok(db.sounds().list()?))
}

/// Import specific files, e.g. from the file picker or a drag and drop.
#[tauri::command]
pub fn import_sounds(state: State<'_, AppState>, paths: Vec<String>) -> Res<ImportReport> {
    let options = options(&state)?;
    let mut report = ImportReport::default();

    state.with_db(|db| {
        for path in &paths {
            match library::import_file(db, &PathBuf::from(path), &options) {
                Ok(sound) => report.imported.push(sound),
                Err(e) => report.skipped.push(crate::library::SkippedFile {
                    path: path.clone(),
                    reason: e.message,
                }),
            }
        }
        Ok::<_, CommandError>(())
    })?;

    tracing::info!(
        imported = report.imported.len(),
        skipped = report.skipped.len(),
        "import finished"
    );
    Ok(report)
}

/// Import every supported file in a folder, recursively.
#[tauri::command]
pub fn import_sound_directory(state: State<'_, AppState>, path: String) -> Res<ImportReport> {
    let options = options(&state)?;
    let report =
        state.with_db(|db| library::import_directory(db, &PathBuf::from(&path), &options))?;

    tracing::info!(
        path = %path,
        imported = report.imported.len(),
        skipped = report.skipped.len(),
        "directory import finished"
    );
    Ok(report)
}

#[tauri::command]
pub fn rename_sound(state: State<'_, AppState>, id: i64, name: String) -> Res<Sound> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(CommandError::new("invalidInput", "Звуку потрібна назва."));
    }
    state.with_db(|db| Ok(db.sounds().rename(id, &name)?))
}

#[tauri::command]
pub fn set_sound_volume(state: State<'_, AppState>, id: i64, volume: f32) -> Res<Sound> {
    if !(0.0..=1.0).contains(&volume) || !volume.is_finite() {
        return Err(CommandError::new(
            "invalidInput",
            "Гучність звуку має бути від 0 до 1.",
        ));
    }
    state.with_db(|db| Ok(db.sounds().set_volume(id, volume)?))
}

#[tauri::command]
pub fn set_sound_weight(state: State<'_, AppState>, id: i64, weight: f32) -> Res<Sound> {
    if !weight.is_finite() || weight < 0.0 || weight > 100.0 {
        return Err(CommandError::new(
            "invalidInput",
            "Вага звуку має бути від 0 до 100.",
        ));
    }
    state.with_db(|db| Ok(db.sounds().set_weight(id, weight)?))
}

#[tauri::command]
pub fn set_sound_enabled(state: State<'_, AppState>, id: i64, enabled: bool) -> Res<Sound> {
    state.with_db(|db| Ok(db.sounds().set_enabled(id, enabled)?))
}

#[tauri::command]
pub fn set_sound_favorite(state: State<'_, AppState>, id: i64, favorite: bool) -> Res<Sound> {
    state.with_db(|db| Ok(db.sounds().set_favorite(id, favorite)?))
}

/// Remove a sound from the library.
///
/// Only the metadata row is deleted. A referenced file is never touched; a managed copy
/// is left in place too, so a mis-click can be undone by re-importing.
#[tauri::command]
pub fn delete_sound(state: State<'_, AppState>, id: i64) -> Res<Vec<Sound>> {
    let path = state.with_db(|db| Ok::<_, CommandError>(db.sounds().get(id)?.file_path))?;
    state.audio().forget_cached(&path);
    state.with_db(|db| {
        db.sounds().delete(id)?;
        Ok(db.sounds().list()?)
    })
}

#[tauri::command]
pub fn sound_tags(state: State<'_, AppState>, id: i64) -> Res<Vec<String>> {
    state.with_db(|db| Ok(db.sounds().tags(id)?))
}

#[tauri::command]
pub fn set_sound_tags(state: State<'_, AppState>, id: i64, tags: Vec<String>) -> Res<Vec<String>> {
    state.with_db(|db| {
        db.sounds().set_tags(id, &tags)?;
        Ok(db.sounds().tags(id)?)
    })
}

/// Re-check every file on disk and flag the ones that have gone.
#[tauri::command]
pub fn rescan_sounds(state: State<'_, AppState>) -> Res<Vec<Sound>> {
    state.with_db(|db| {
        for sound in db.sounds().list()? {
            let missing = !std::path::Path::new(&sound.file_path).is_file();
            if missing != sound.missing {
                db.sounds().set_missing(sound.id, missing)?;
                if missing {
                    tracing::warn!(path = %sound.file_path, "sound file has disappeared");
                }
            }
        }
        Ok(db.sounds().list()?)
    })
}

// ---------------------------------------------------------------------- groups --

#[tauri::command]
pub fn list_sound_groups(state: State<'_, AppState>) -> Res<Vec<SoundGroup>> {
    state.with_db(|db| Ok(db.sounds().list_groups()?))
}

#[tauri::command]
pub fn create_sound_group(state: State<'_, AppState>, name: String) -> Res<SoundGroup> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(CommandError::new("invalidInput", "Групі потрібна назва."));
    }
    state.with_db(|db| Ok(db.sounds().create_group(&name)?))
}

#[tauri::command]
pub fn update_sound_group(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    selection_mode: String,
    prevent_repeat: bool,
    volume: f32,
) -> Res<SoundGroup> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(CommandError::new("invalidInput", "Групі потрібна назва."));
    }
    if !matches!(
        selection_mode.as_str(),
        "random" | "weighted" | "sequential"
    ) {
        return Err(CommandError::new(
            "invalidInput",
            format!("Невідомий режим вибору «{selection_mode}»."),
        ));
    }
    if !(0.0..=1.0).contains(&volume) || !volume.is_finite() {
        return Err(CommandError::new(
            "invalidInput",
            "Гучність групи має бути від 0 до 1.",
        ));
    }

    let group = state.with_db(|db| {
        Ok::<_, CommandError>(db.sounds().update_group(
            id,
            &name,
            &selection_mode,
            prevent_repeat,
            volume,
        )?)
    })?;

    // Selection settings changed, so the remembered play order no longer applies.
    state.audio().reset_group(id);
    Ok(group)
}

#[tauri::command]
pub fn delete_sound_group(state: State<'_, AppState>, id: i64) -> Res<Vec<SoundGroup>> {
    state.audio().reset_group(id);
    state.with_db(|db| {
        db.sounds().delete_group(id)?;
        Ok(db.sounds().list_groups()?)
    })
}

#[tauri::command]
pub fn sound_group_members(state: State<'_, AppState>, id: i64) -> Res<Vec<Sound>> {
    state.with_db(|db| Ok(db.sounds().group_members(id)?))
}

#[tauri::command]
pub fn add_sound_to_group(
    state: State<'_, AppState>,
    group_id: i64,
    sound_id: i64,
) -> Res<Vec<Sound>> {
    state.audio().reset_group(group_id);
    state.with_db(|db| {
        db.sounds().add_to_group(group_id, sound_id)?;
        Ok(db.sounds().group_members(group_id)?)
    })
}

#[tauri::command]
pub fn remove_sound_from_group(
    state: State<'_, AppState>,
    group_id: i64,
    sound_id: i64,
) -> Res<Vec<Sound>> {
    state.audio().reset_group(group_id);
    state.with_db(|db| {
        db.sounds().remove_from_group(group_id, sound_id)?;
        Ok(db.sounds().group_members(group_id)?)
    })
}
