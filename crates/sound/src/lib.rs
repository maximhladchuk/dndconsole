//! The sound engine: turns a decision to play something into audible output.
//!
//! Deliberately separate from detection. This crate is handed a file and a gain and
//! plays it; it never decides *whether* a sound should happen. That split is what makes
//! both halves testable — see `docs/ARCHITECTURE.md`.

mod cache;
mod error;
mod probe;
mod rng;
mod selection;

pub use error::{Error, Result};
pub use probe::{is_supported, probe, SoundMetadata, SUPPORTED_EXTENSIONS};
pub use rng::Rng;
pub use selection::{Candidate, GroupSelector, SelectionMode};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use kira::backend::DefaultBackend;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::sound::{FromFileError, PlaybackState, Region};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, Easing, Tween};

use crate::cache::ByteBudgetCache;

/// Default decode cache budget. 64 MB holds a few hundred typical one-shots while
/// staying far below what a long session can afford to leak.
pub const DEFAULT_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// Logical output buses, as required by the spec.
///
/// MUSIC and VOICE exist as first-class routes from the start so that adding them later
/// is a UI change rather than an audio-graph change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bus {
    Sfx,
    Ambience,
    Music,
    Voice,
}

impl Bus {
    fn name(self) -> &'static str {
        match self {
            Bus::Sfx => "sfx",
            Bus::Ambience => "ambience",
            Bus::Music => "music",
            Bus::Voice => "voice",
        }
    }
}

/// Linear volumes in `0.0..=1.0`, as stored in settings and shown in the UI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volumes {
    pub master: f32,
    pub sfx: f32,
    pub ambience: f32,
    pub music: f32,
    pub voice: f32,
    pub muted: bool,
}

impl Default for Volumes {
    fn default() -> Self {
        Self {
            master: 1.0,
            sfx: 1.0,
            ambience: 0.7,
            music: 0.7,
            voice: 1.0,
            muted: false,
        }
    }
}

/// Convert a linear amplitude to decibels.
///
/// Volume sliders are linear because that is what users expect to see; the mixer works
/// in decibels. Anything at or below -60 dB is treated as silence.
fn to_decibels(linear: f32) -> Decibels {
    if !linear.is_finite() || linear <= 0.001 {
        Decibels::SILENCE
    } else {
        Decibels::from(20.0 * linear.log10())
    }
}

fn tween(duration: Duration) -> Tween {
    Tween {
        duration,
        easing: Easing::Linear,
        ..Default::default()
    }
}

/// A one-shot that is currently audible.
struct ActiveOneShot {
    handle: StaticSoundHandle,
    path: PathBuf,
}

pub struct SoundEngine {
    manager: AudioManager<DefaultBackend>,
    tracks: HashMap<Bus, TrackHandle>,
    volumes: Volumes,

    /// Decoded one-shots, bounded by bytes. Ambience is never cached — it streams.
    cache: ByteBudgetCache<PathBuf, StaticSoundData>,

    one_shots: Vec<ActiveOneShot>,
    ambience: HashMap<String, StreamingSoundHandle<FromFileError>>,

    rng: Rng,
}

impl SoundEngine {
    /// Open the default output device and build the mixer graph.
    pub fn new() -> Result<Self> {
        Self::with_cache_budget(DEFAULT_CACHE_BUDGET_BYTES)
    }

    pub fn with_cache_budget(budget_bytes: usize) -> Result<Self> {
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| Error::Backend(e.to_string()))?;

        let mut tracks = HashMap::new();
        for bus in [Bus::Sfx, Bus::Ambience, Bus::Music, Bus::Voice] {
            let track = manager
                .add_sub_track(TrackBuilder::new())
                .map_err(|source| Error::Track {
                    track: bus.name().to_string(),
                    source,
                })?;
            tracks.insert(bus, track);
        }

        let mut engine = Self {
            manager,
            tracks,
            volumes: Volumes::default(),
            cache: ByteBudgetCache::new(budget_bytes, |data: &StaticSoundData| {
                // Each frame is a stereo pair of f32 samples.
                data.num_frames() * std::mem::size_of::<f32>() * 2
            }),
            one_shots: Vec::new(),
            ambience: HashMap::new(),
            rng: Rng::from_entropy(),
        };

        engine.apply_volumes();
        Ok(engine)
    }

    /// Replace the volume settings, applying them immediately with a short tween so
    /// dragging a slider does not click.
    pub fn set_volumes(&mut self, volumes: Volumes) {
        self.volumes = volumes;
        self.apply_volumes();
    }

    pub fn volumes(&self) -> Volumes {
        self.volumes
    }

    fn apply_volumes(&mut self) {
        let ramp = tween(Duration::from_millis(30));
        let master = if self.volumes.muted {
            0.0
        } else {
            self.volumes.master
        };

        self.manager
            .main_track()
            .set_volume(to_decibels(master), ramp);

        for (bus, track) in self.tracks.iter_mut() {
            let level = match bus {
                Bus::Sfx => self.volumes.sfx,
                Bus::Ambience => self.volumes.ambience,
                Bus::Music => self.volumes.music,
                Bus::Voice => self.volumes.voice,
            };
            track.set_volume(to_decibels(level), ramp);
        }
    }

    /// Play a file once. Overlaps freely with everything already playing.
    ///
    /// `gain` is the sound's own linear volume (file volume × group volume); the bus
    /// and master levels are applied by the mixer on top of it.
    pub fn play_one_shot(&mut self, path: impl AsRef<Path>, gain: f32, bus: Bus) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        let data = self.load(&path)?.volume(to_decibels(gain));

        let track = self
            .tracks
            .get_mut(&bus)
            .expect("every bus has a track from construction");

        let handle = track.play(data).map_err(|e| Error::Play {
            path: path.clone(),
            reason: e.to_string(),
        })?;

        self.prune_finished();
        self.one_shots.push(ActiveOneShot { handle, path });
        Ok(())
    }

    /// Start (or restart) a looping ambience bed, streamed from disk rather than
    /// decoded into memory.
    ///
    /// `key` identifies the bed, so starting the same key twice crossfades rather than
    /// stacking two copies of the rain.
    pub fn start_ambience(
        &mut self,
        key: impl Into<String>,
        path: impl AsRef<Path>,
        gain: f32,
        fade_in: Duration,
    ) -> Result<()> {
        let key = key.into();
        let path = path.as_ref().to_path_buf();

        if !path.exists() {
            return Err(Error::Missing(path));
        }

        let data = StreamingSoundData::from_file(&path)
            .map_err(|source| Error::Decode {
                path: path.clone(),
                source,
            })?
            .volume(to_decibels(gain))
            .loop_region(Region::from(..))
            .fade_in_tween(tween(fade_in));

        // Fade the outgoing bed rather than cutting it.
        if let Some(mut previous) = self.ambience.remove(&key) {
            previous.stop(tween(fade_in.max(Duration::from_millis(200))));
        }

        let track = self
            .tracks
            .get_mut(&Bus::Ambience)
            .expect("ambience track exists from construction");

        let handle = track.play(data).map_err(|e| Error::Play {
            path: path.clone(),
            reason: e.to_string(),
        })?;

        self.ambience.insert(key, handle);
        Ok(())
    }

    /// Fade out and stop one ambience bed. Unknown keys are a no-op.
    pub fn stop_ambience(&mut self, key: &str, fade_out: Duration) {
        if let Some(mut handle) = self.ambience.remove(key) {
            handle.stop(tween(fade_out));
        }
    }

    pub fn stop_all_ambience(&mut self, fade_out: Duration) {
        let keys: Vec<String> = self.ambience.keys().cloned().collect();
        for key in keys {
            self.stop_ambience(&key, fade_out);
        }
    }

    /// Stop everything immediately-ish. Used when a session ends.
    pub fn stop_all(&mut self, fade_out: Duration) {
        for active in &mut self.one_shots {
            active.handle.stop(tween(fade_out));
        }
        self.one_shots.clear();
        self.stop_all_ambience(fade_out);
    }

    /// Number of one-shots still audible, after dropping finished handles.
    pub fn active_one_shots(&mut self) -> usize {
        self.prune_finished();
        self.one_shots.len()
    }

    pub fn active_ambience(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.ambience.keys().cloned().collect();
        keys.sort();
        keys
    }

    pub fn is_ambience_playing(&self, key: &str) -> bool {
        self.ambience.contains_key(key)
    }

    pub fn cache_used_bytes(&self) -> usize {
        self.cache.used_bytes()
    }

    pub fn invalidate_cached(&mut self, path: impl AsRef<Path>) {
        self.cache.invalidate(&path.as_ref().to_path_buf());
    }

    pub fn rng(&mut self) -> &mut Rng {
        &mut self.rng
    }

    /// Decode a file, reusing the cached copy when there is one.
    ///
    /// `StaticSoundData` is cheap to clone — the samples live behind an `Arc` — so a
    /// cache hit costs a refcount bump rather than a decode.
    fn load(&mut self, path: &PathBuf) -> Result<StaticSoundData> {
        if let Some(cached) = self.cache.get(path) {
            return Ok(cached.clone());
        }

        if !path.exists() {
            return Err(Error::Missing(path.clone()));
        }

        let data = StaticSoundData::from_file(path).map_err(|source| Error::Decode {
            path: path.clone(),
            source,
        })?;

        self.cache.insert(path.clone(), data.clone());
        Ok(data)
    }

    fn prune_finished(&mut self) {
        self.one_shots
            .retain(|active| active.handle.state() != PlaybackState::Stopped);
    }

    /// Paths of the one-shots currently playing. Diagnostic only.
    pub fn playing_paths(&mut self) -> Vec<PathBuf> {
        self.prune_finished();
        self.one_shots.iter().map(|a| a.path.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_and_unity_map_to_the_expected_decibels() {
        assert_eq!(to_decibels(0.0), Decibels::SILENCE);
        assert_eq!(to_decibels(-1.0), Decibels::SILENCE);
        assert_eq!(to_decibels(f32::NAN), Decibels::SILENCE);

        let unity = to_decibels(1.0).as_amplitude();
        assert!(
            (unity - 1.0).abs() < 0.001,
            "1.0 linear should be unity gain"
        );
    }

    #[test]
    fn halving_the_linear_volume_is_about_minus_six_decibels() {
        let half = to_decibels(0.5).as_amplitude();
        assert!((half - 0.5).abs() < 0.01, "round trip drifted: {half}");
    }
}
