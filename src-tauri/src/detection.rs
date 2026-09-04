//! Detection state: the event definitions currently loaded, and what to do when one fires.
//!
//! The detector is rebuilt whenever events change, and read-locked for every transcript.
//! Rebuilding is cheap (it re-normalizes phrases) and happens in the editor, not on the
//! hot path.

use std::sync::{Arc, Mutex, RwLock};

use dndsound_detect::{
    AlwaysRoll, Decision, Detection, DetectionInput, Detector, Roll, SemanticScorer, TriggerEngine,
    TriggerRules,
};
use dndsound_models::ModelStore;
use dndsound_semantic::Embedder;
use dndsound_store::Db;

use crate::error::CommandError;

/// Deterministic-enough randomness for probability rolls.
///
/// The same tiny generator the sound engine uses, for the same reason: no cryptography
/// is involved, and a dependency whose API churns is not worth it here.
struct SessionRoll(dndsound_sound::Rng);

impl Roll for SessionRoll {
    fn next(&mut self) -> f32 {
        self.0.next_f32()
    }
}

pub struct DetectionState {
    detector: RwLock<Detector>,
    rules: RwLock<Vec<TriggerRules>>,
    triggers: Mutex<TriggerEngine>,
    roll: Mutex<SessionRoll>,
    /// Present only when the embedding model has been downloaded.
    embedder: RwLock<Option<Arc<Embedder>>>,
}

impl DetectionState {
    /// Build from whatever is in the database right now.
    pub fn load(db: &Db) -> Result<Self, CommandError> {
        let (detector, rules) = build(db)?;
        Ok(Self {
            detector: RwLock::new(detector),
            rules: RwLock::new(rules),
            triggers: Mutex::new(TriggerEngine::new()),
            roll: Mutex::new(SessionRoll(dndsound_sound::Rng::from_entropy())),
            embedder: RwLock::new(None),
        })
    }

    /// Load the embedding model, if it has been downloaded, and index the phrases.
    ///
    /// Safe to call repeatedly: it is how the semantic layer comes online right after
    /// the model finishes downloading, without a restart.
    pub fn attach_semantic(&self, models: &ModelStore, db: &Arc<Mutex<Db>>) {
        let embedder = match self
            .embedder
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            Some(embedder) => Some(embedder),
            None => crate::semantic::load_embedder(models),
        };

        let Some(embedder) = embedder else { return };
        *self.embedder.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&embedder));

        let definitions: Vec<dndsound_detect::EventDefinition> = {
            let guard = db.lock().unwrap_or_else(|e| e.into_inner());
            match guard.events().list() {
                Ok(events) => events.into_iter().map(|event| event.definition).collect(),
                Err(e) => {
                    tracing::error!(error = %e, "could not read events for the semantic index");
                    return;
                }
            }
        };

        if let Some(index) = crate::semantic::build_index(embedder, Arc::clone(db), &definitions) {
            self.detector
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .set_semantic(Some(index as Arc<dyn SemanticScorer>));
        }
    }

    pub fn has_semantic(&self) -> bool {
        self.detector
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .has_semantic()
    }

    /// Rebuild after the event editor changes something.
    ///
    /// The semantic index is rebuilt too, because a new phrase that is not indexed would
    /// silently never match semantically.
    pub fn reload(&self, db: &Db) -> Result<(), CommandError> {
        let (mut detector, rules) = build(db)?;

        let embedder = self
            .embedder
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(embedder) = embedder {
            let definitions: Vec<dndsound_detect::EventDefinition> = db
                .events()
                .list()?
                .into_iter()
                .map(|event| event.definition)
                .collect();

            match dndsound_semantic::SemanticEventIndex::build(embedder, &definitions) {
                Ok(index) => {
                    detector.set_semantic(Some(Arc::new(index) as Arc<dyn SemanticScorer>))
                }
                Err(e) => tracing::error!(error = %e, "could not rebuild the semantic index"),
            }
        }

        *self.detector.write().unwrap_or_else(|e| e.into_inner()) = detector;
        *self.rules.write().unwrap_or_else(|e| e.into_inner()) = rules;
        Ok(())
    }

    pub fn set_sensitivity(&self, sensitivity: f32) {
        self.detector
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .set_sensitivity(sensitivity);
    }

    pub fn event_count(&self) -> usize {
        self.detector
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .event_count()
    }

    /// Score a transcript. Does not decide whether to play anything.
    pub fn detect(&self, transcript: &str, is_final: bool, at_ms: i64) -> Detection {
        let detector = self.detector.read().unwrap_or_else(|e| e.into_inner());
        detector.detect(DetectionInput {
            transcript,
            is_final,
            timestamp_ms: at_ms,
        })
    }

    /// Decide which accepted candidates actually fire, applying cooldowns, duplicate
    /// suppression and probability.
    pub fn decide(&self, detection: &Detection) -> Decision {
        let rules = self.rules.read().unwrap_or_else(|e| e.into_inner());
        let mut triggers = self.triggers.lock().unwrap_or_else(|e| e.into_inner());
        let mut roll = self.roll.lock().unwrap_or_else(|e| e.into_inner());
        triggers.decide(detection, &rules, &mut *roll)
    }

    /// Decide without rolling probability, for the simulator: a simulated line should
    /// show what *would* happen, not lose to a dice roll.
    pub fn decide_deterministically(&self, detection: &Detection) -> Decision {
        let rules = self.rules.read().unwrap_or_else(|e| e.into_inner());
        let mut triggers = self.triggers.lock().unwrap_or_else(|e| e.into_inner());
        triggers.decide(detection, &rules, &mut AlwaysRoll)
    }

    /// Forget cooldowns and recent triggers, at the start of a session.
    pub fn reset_history(&self) {
        self.triggers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reset();
    }
}

fn build(db: &Db) -> Result<(Detector, Vec<TriggerRules>), CommandError> {
    let stored = db.events().list()?;

    let rules = stored
        .iter()
        .map(|event| {
            TriggerRules::new(&event.definition.id, event.definition.cooldown_ms)
                .with_probability(event.definition.probability)
        })
        .collect();

    let definitions = stored.into_iter().map(|event| event.definition).collect();
    let mut detector = Detector::new(definitions);

    let sensitivity = db.settings().load()?.event_sensitivity;
    detector.set_sensitivity(sensitivity);

    Ok((detector, rules))
}

/// Play whatever a trigger asks for, resolving the event to its sound group.
///
/// Returns the sound that played, if one did. An event with no sound group assigned is a
/// configuration gap, not an error: the UI shows it, and detection still works.
pub fn play_trigger(
    db: &Arc<Mutex<Db>>,
    audio: &crate::audio::AudioState,
    event_id: &str,
) -> Result<Option<dndsound_store::sounds::Sound>, CommandError> {
    let guard = db.lock().unwrap_or_else(|e| e.into_inner());

    let event = guard.events().get(event_id)?;
    let Some(group_id) = event.sound_group_id else {
        tracing::debug!(event = event_id, "no sound group assigned");
        return Ok(None);
    };

    let group = guard.sounds().group(group_id)?;
    let members = guard.sounds().group_members(group_id)?;
    drop(guard);

    let bus = match event.track.as_str() {
        "ambience" => dndsound_sound::Bus::Ambience,
        "music" => dndsound_sound::Bus::Music,
        "voice" => dndsound_sound::Bus::Voice,
        _ => dndsound_sound::Bus::Sfx,
    };

    audio.play_group(&group, &members, bus)
}
