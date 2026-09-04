//! Event editor commands.

use dndsound_detect::EventDefinition;
use dndsound_store::events::StoredEvent;
use tauri::State;

use crate::commands::app::Res;
use crate::error::CommandError;
use crate::state::AppState;

#[tauri::command]
pub fn list_events(state: State<'_, AppState>) -> Res<Vec<StoredEvent>> {
    state.with_db(|db| Ok(db.events().list()?))
}

#[tauri::command]
pub fn get_event(state: State<'_, AppState>, id: String) -> Res<StoredEvent> {
    state.with_db(|db| Ok(db.events().get(&id)?))
}

/// Create or update an event, then rebuild the detector so the change takes effect
/// immediately — including mid-session.
#[tauri::command]
pub fn save_event(
    state: State<'_, AppState>,
    definition: EventDefinition,
    sound_group_id: Option<i64>,
    track: String,
) -> Res<StoredEvent> {
    validate(&definition)?;

    let track = match track.as_str() {
        "sfx" | "ambience" | "music" | "voice" => track,
        other => {
            return Err(CommandError::new(
                "invalidInput",
                format!("Unknown track '{other}'."),
            ))
        }
    };

    let stored = state.with_db(|db| {
        db.events().upsert(&definition, sound_group_id, &track)?;
        // From here on the seed stops rewriting this event on startup. The user's
        // phrasing outranks ours, even when ours later improves.
        db.events().mark_user_modified(&definition.id)?;
        Ok::<_, CommandError>(db.events().get(&definition.id)?)
    })?;

    reload_detector(&state)?;
    Ok(stored)
}

/// Throw away a person's edits to a built-in event and take the shipped definition.
#[tauri::command]
pub fn reset_event(state: State<'_, AppState>, id: String) -> Res<StoredEvent> {
    let stored = state.with_db(|db| {
        db.events().reset_to_builtin(&id)?;
        Ok::<_, CommandError>(db.events().get(&id)?)
    })?;

    reload_detector(&state)?;
    Ok(stored)
}

#[tauri::command]
pub fn set_event_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Res<StoredEvent> {
    let stored = state.with_db(|db| {
        db.events().set_enabled(&id, enabled)?;
        Ok::<_, CommandError>(db.events().get(&id)?)
    })?;

    reload_detector(&state)?;
    Ok(stored)
}

#[tauri::command]
pub fn set_event_sound_group(
    state: State<'_, AppState>,
    id: String,
    sound_group_id: Option<i64>,
) -> Res<StoredEvent> {
    let stored = state.with_db(|db| {
        db.events().set_sound_group(&id, sound_group_id)?;
        Ok::<_, CommandError>(db.events().get(&id)?)
    })?;

    reload_detector(&state)?;
    Ok(stored)
}

#[tauri::command]
pub fn delete_event(state: State<'_, AppState>, id: String) -> Res<Vec<StoredEvent>> {
    let events = state.with_db(|db| {
        db.events().delete(&id)?;
        Ok::<_, CommandError>(db.events().list()?)
    })?;

    reload_detector(&state)?;
    Ok(events)
}

/// Restore every built-in event, discarding edits, without touching user-created ones.
#[tauri::command]
pub fn restore_seed_events(state: State<'_, AppState>) -> Res<Vec<StoredEvent>> {
    let events = state.with_db(|db| {
        for event in dndsound_detect::seed_events() {
            // Existing events keep their sound group; only the rules are restored.
            db.events().reset_to_builtin(&event.id)?;
        }
        Ok::<_, CommandError>(db.events().list()?)
    })?;

    reload_detector(&state)?;
    Ok(events)
}

fn reload_detector(state: &State<'_, AppState>) -> Res<()> {
    state.with_db(|db| state.detection().reload(db))
}

/// Reject definitions that would behave nonsensically.
fn validate(definition: &EventDefinition) -> Res<()> {
    let id = definition.id.trim();
    if id.is_empty() {
        return Err(CommandError::new(
            "invalidInput",
            "An event needs an identifier.",
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(CommandError::new(
            "invalidInput",
            "Event identifiers may only contain letters, digits, underscores and hyphens.",
        ));
    }
    if definition.display_name.trim().is_empty() {
        return Err(CommandError::new("invalidInput", "An event needs a name."));
    }
    if !(0.0..=1.0).contains(&definition.confidence_threshold)
        || !definition.confidence_threshold.is_finite()
    {
        return Err(CommandError::new(
            "invalidInput",
            "Confidence threshold must be between 0 and 1.",
        ));
    }
    if !(0.0..=1.0).contains(&definition.probability) || !definition.probability.is_finite() {
        return Err(CommandError::new(
            "invalidInput",
            "Probability must be between 0 and 1.",
        ));
    }
    if definition.cooldown_ms > 600_000 {
        return Err(CommandError::new(
            "invalidInput",
            "A cooldown longer than ten minutes is almost certainly a mistake.",
        ));
    }
    if definition.phrases.is_empty() && definition.terms.is_empty() {
        return Err(CommandError::new(
            "invalidInput",
            "An event needs at least one phrase or keyword, or it can never fire.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dndsound_detect::{Lang, Phrase, Term};

    fn usable() -> EventDefinition {
        EventDefinition::new("OPEN_DOOR", "Open Door")
            .with_phrases(vec![Phrase::example(Lang::En, "opens the door")])
            .with_terms(vec![Term::keyword("door")])
    }

    #[test]
    fn a_well_formed_event_is_accepted() {
        assert!(validate(&usable()).is_ok());
    }

    #[test]
    fn an_event_with_nothing_to_match_on_is_rejected() {
        let empty = EventDefinition::new("EMPTY", "Empty");
        let err = validate(&empty).expect_err("should be rejected");
        assert!(err.message.contains("never fire"), "got {}", err.message);
    }

    #[test]
    fn identifiers_are_restricted() {
        let mut event = usable();
        event.id = "open door!".to_string();
        assert!(validate(&event).is_err());

        event.id = "  ".to_string();
        assert!(validate(&event).is_err());

        event.id = "OPEN_DOOR-2".to_string();
        assert!(validate(&event).is_ok());
    }

    #[test]
    fn out_of_range_thresholds_and_probabilities_are_rejected() {
        let mut event = usable();
        event.confidence_threshold = 1.5;
        assert!(validate(&event).is_err());

        let mut event = usable();
        event.confidence_threshold = f32::NAN;
        assert!(validate(&event).is_err());

        let mut event = usable();
        event.probability = -0.2;
        assert!(validate(&event).is_err());
    }

    #[test]
    fn an_absurd_cooldown_is_rejected() {
        let mut event = usable();
        event.cooldown_ms = 3_600_000;
        assert!(validate(&event).is_err());
    }
}
