//! Application-level commands: status, settings, campaign profiles.

use dndsound_store::{profiles::Profile, AppSettings};
use serde::Serialize;
use tauri::State;

use crate::{error::CommandError, state::AppState};

pub type Res<T> = Result<T, CommandError>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub version: String,
    pub database_path: String,
    pub schema_version: Option<u32>,
    pub active_profile: Option<Profile>,
    pub audio: crate::audio::PlaybackSnapshot,
}

#[tauri::command]
pub fn app_status(state: State<'_, AppState>) -> Res<AppStatus> {
    state.with_db(|db| {
        Ok(AppStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            database_path: state.db_path().display().to_string(),
            schema_version: db.schema_version()?,
            active_profile: db.profiles().active()?,
            audio: state.audio().snapshot(),
        })
    })
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Res<AppSettings> {
    state.with_db(|db| Ok(db.settings().load()?))
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Res<AppSettings> {
    validate_settings(&settings)?;
    let saved = state.with_db(|db| {
        db.settings().save(&settings)?;
        Ok::<_, CommandError>(db.settings().load()?)
    })?;

    // Volume and mute changes are only real once the mixer hears about them. An audio
    // device that is unavailable must not block saving settings, so the failure is
    // logged rather than returned.
    state.detection().set_sensitivity(saved.event_sensitivity);

    // Takes effect immediately, session running or not, so the checkbox can be tried
    // against a sound without restarting listening.
    state
        .session()
        .set_suppress_playback(saved.suppress_mic_during_playback);

    if let Err(e) = state.audio().apply_settings(&saved) {
        tracing::warn!(error = %e, "settings saved but could not be applied to the audio engine");
    }

    Ok(saved)
}

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> Res<Vec<Profile>> {
    state.with_db(|db| Ok(db.profiles().list()?))
}

#[tauri::command]
pub fn create_profile(
    state: State<'_, AppState>,
    name: String,
    description: String,
) -> Res<Profile> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalidInput",
            "Profile name cannot be empty.",
        ));
    }
    state.with_db(|db| Ok(db.profiles().create(&name, description.trim())?))
}

#[tauri::command]
pub fn set_active_profile(state: State<'_, AppState>, id: i64) -> Res<Profile> {
    state.with_db(|db| {
        db.profiles().set_active(id)?;
        Ok(db.profiles().get(id)?)
    })
}

#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, id: i64) -> Res<Vec<Profile>> {
    state.with_db(|db| {
        db.profiles().delete(id)?;
        // Deleting the active profile must not leave the app with none active.
        db.profiles().ensure_default()?;
        Ok(db.profiles().list()?)
    })
}

/// Reject values that would produce nonsensical audio or detection behaviour.
///
/// Bounds are enforced here rather than only in the UI, since commands are a trust
/// boundary: the frontend is not the only thing that could ever call them.
fn validate_settings(s: &AppSettings) -> Res<()> {
    fn unit(name: &str, value: f32) -> Res<()> {
        if !(0.0..=1.0).contains(&value) || !value.is_finite() {
            return Err(CommandError::new(
                "invalidInput",
                format!("{name} must be between 0 and 1 (got {value})."),
            ));
        }
        Ok(())
    }

    unit("Master volume", s.master_volume)?;
    unit("SFX volume", s.sfx_volume)?;
    unit("Ambience volume", s.ambience_volume)?;
    unit("Event sensitivity", s.event_sensitivity)?;
    unit("VAD speech threshold", s.vad_speech_threshold)?;

    if !matches!(s.language.as_str(), "auto" | "uk" | "en") {
        return Err(CommandError::new(
            "invalidInput",
            format!(
                "Unsupported language '{}'. Expected auto, uk or en.",
                s.language
            ),
        ));
    }

    if !(1_000..=60_000).contains(&s.vad_max_segment_ms) {
        return Err(CommandError::new(
            "invalidInput",
            "The monologue cut must be between 1 and 60 seconds.",
        ));
    }

    if s.vad_min_speech_ms > s.vad_max_segment_ms {
        return Err(CommandError::new(
            "invalidInput",
            "Minimum speech duration cannot exceed the maximum segment length.",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert!(validate_settings(&AppSettings::default()).is_ok());
    }

    #[test]
    fn out_of_range_volume_is_rejected() {
        let too_loud = AppSettings {
            master_volume: 1.5,
            ..AppSettings::default()
        };
        assert!(validate_settings(&too_loud).is_err());

        let not_a_number = AppSettings {
            master_volume: f32::NAN,
            ..AppSettings::default()
        };
        assert!(validate_settings(&not_a_number).is_err());
    }

    #[test]
    fn unknown_language_is_rejected() {
        let s = AppSettings {
            language: "klingon".to_string(),
            ..AppSettings::default()
        };
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn contradictory_vad_bounds_are_rejected() {
        let s = AppSettings {
            vad_min_speech_ms: 20_000,
            vad_max_segment_ms: 15_000,
            ..AppSettings::default()
        };
        assert!(validate_settings(&s).is_err());
    }
}
