//! Playback state for the running application.
//!
//! Wraps the sound engine with the bits that need database context: resolving a group
//! to its members, remembering per-group selection state, and applying the user's
//! volume settings.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use dndsound_sound::{
    Bus, Candidate, GroupSelector, SelectionMode, SoundEngine, Volumes, DEFAULT_CACHE_BUDGET_BYTES,
};
use dndsound_store::sounds::{Sound, SoundGroup};
use dndsound_store::AppSettings;

use crate::error::CommandError;

/// Default ambience crossfade. Long enough not to click, short enough to feel responsive.
pub const AMBIENCE_FADE: Duration = Duration::from_millis(600);

pub struct AudioState {
    /// `None` when no output device could be opened. The app still runs — the library
    /// and event editor are useful without audio — but playback commands explain why
    /// they cannot work instead of failing silently.
    inner: Mutex<Option<Engine>>,
    unavailable_reason: Option<String>,
}

struct Engine {
    engine: SoundEngine,
    selectors: HashMap<i64, GroupSelector>,
}

impl AudioState {
    pub fn initialize(settings: &AppSettings) -> Self {
        match SoundEngine::with_cache_budget(DEFAULT_CACHE_BUDGET_BYTES) {
            Ok(mut engine) => {
                engine.set_volumes(volumes_from(settings));
                tracing::info!("audio output ready");
                Self {
                    inner: Mutex::new(Some(Engine {
                        engine,
                        selectors: HashMap::new(),
                    })),
                    unavailable_reason: None,
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "audio output unavailable");
                Self {
                    inner: Mutex::new(None),
                    unavailable_reason: Some(e.to_string()),
                }
            }
        }
    }

    fn with<T>(
        &self,
        f: impl FnOnce(&mut Engine) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        match guard.as_mut() {
            Some(engine) => f(engine),
            None => Err(CommandError::new(
                "audioUnavailable",
                self.unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "Немає доступного пристрою відтворення звуку.".to_string()),
            )),
        }
    }

    pub fn apply_settings(&self, settings: &AppSettings) -> Result<(), CommandError> {
        self.with(|state| {
            state.engine.set_volumes(volumes_from(settings));
            Ok(())
        })
    }

    /// Play one specific file, e.g. the preview button in the library.
    pub fn preview(&self, sound: &Sound) -> Result<(), CommandError> {
        self.with(|state| {
            state
                .engine
                .play_one_shot(&sound.file_path, sound.volume, Bus::Sfx)
                .map_err(playback_error)
        })
    }

    /// Play one sound chosen from `group` according to its selection settings.
    ///
    /// Returns the sound that was played, so the UI and the session log can show which
    /// file the group picked.
    pub fn play_group(
        &self,
        group: &SoundGroup,
        members: &[Sound],
        bus: Bus,
    ) -> Result<Option<Sound>, CommandError> {
        let playable: Vec<&Sound> = members.iter().filter(|s| s.enabled && !s.missing).collect();

        if playable.is_empty() {
            return Ok(None);
        }

        let candidates: Vec<Candidate> = playable
            .iter()
            .map(|s| Candidate::new(s.id, s.weight))
            .collect();

        self.with(|state| {
            let selector = state
                .selectors
                .entry(group.id)
                .or_insert_with(|| GroupSelector::new(mode_of(group), group.prevent_repeat));

            // Group settings can change in the editor while the app runs; rebuild the
            // selector when they do, rather than honouring stale settings.
            if selector.mode() != mode_of(group) {
                *selector = GroupSelector::new(mode_of(group), group.prevent_repeat);
            }

            let chosen_id = match selector.select(&candidates, state.engine.rng()) {
                Some(id) => id,
                None => return Ok(None),
            };

            let chosen = playable
                .iter()
                .find(|s| s.id == chosen_id)
                .expect("the selector only returns ids it was given");

            state
                .engine
                .play_one_shot(&chosen.file_path, chosen.volume * group.volume, bus)
                .map_err(playback_error)?;

            Ok(Some((*chosen).clone()))
        })
    }

    pub fn start_ambience(&self, key: &str, sound: &Sound) -> Result<(), CommandError> {
        self.with(|state| {
            state
                .engine
                .start_ambience(key, &sound.file_path, sound.volume, AMBIENCE_FADE)
                .map_err(playback_error)
        })
    }

    pub fn stop_ambience(&self, key: &str) -> Result<(), CommandError> {
        self.with(|state| {
            state.engine.stop_ambience(key, AMBIENCE_FADE);
            Ok(())
        })
    }

    pub fn stop_all(&self) -> Result<(), CommandError> {
        self.with(|state| {
            state.engine.stop_all(Duration::from_millis(120));
            Ok(())
        })
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        match guard.as_mut() {
            Some(state) => PlaybackSnapshot {
                available: true,
                unavailable_reason: None,
                active_one_shots: state.engine.active_one_shots(),
                active_ambience: state.engine.active_ambience(),
                cache_used_bytes: state.engine.cache_used_bytes(),
            },
            None => PlaybackSnapshot {
                available: false,
                unavailable_reason: self.unavailable_reason.clone(),
                active_one_shots: 0,
                active_ambience: Vec::new(),
                cache_used_bytes: 0,
            },
        }
    }

    /// Drop a file from the decode cache, e.g. after it was deleted or replaced.
    pub fn forget_cached(&self, path: &str) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(state) = guard.as_mut() {
            state.engine.invalidate_cached(path);
        }
    }

    /// Forget a group's play history after its contents change.
    pub fn reset_group(&self, group_id: i64) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(state) = guard.as_mut() {
            state.selectors.remove(&group_id);
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSnapshot {
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub active_one_shots: usize,
    pub active_ambience: Vec<String>,
    pub cache_used_bytes: usize,
}

fn volumes_from(settings: &AppSettings) -> Volumes {
    Volumes {
        master: settings.master_volume,
        sfx: settings.sfx_volume,
        ambience: settings.ambience_volume,
        music: settings.ambience_volume,
        voice: settings.master_volume,
        muted: settings.effects_muted,
    }
}

fn mode_of(group: &SoundGroup) -> SelectionMode {
    match group.selection_mode.as_str() {
        "weighted" => SelectionMode::Weighted,
        "sequential" => SelectionMode::Sequential,
        _ => SelectionMode::Random,
    }
}

fn playback_error(err: dndsound_sound::Error) -> CommandError {
    use dndsound_sound::Error as E;
    let kind = match err {
        E::Missing(_) => "soundFileMissing",
        E::Decode { .. } => "soundDecodeFailed",
        E::Backend(_) | E::Track { .. } => "audioUnavailable",
        E::Play { .. } => "playbackFailed",
    };
    tracing::error!(error = %err, "playback error");
    CommandError::new(kind, err.to_string())
}
