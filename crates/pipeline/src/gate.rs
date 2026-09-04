//! Muting the microphone while the application's own sounds are playing.
//!
//! Speakers and a microphone in the same room form a loop: a thunderclap the app just
//! played is heard by the microphone, recognised as words, and can trigger another
//! sound. There is no acoustic echo canceller here — the app does not know what the
//! speakers are actually emitting — so the loop is broken the blunt way, by ignoring
//! the microphone for as long as a sound is audible.
//!
//! The cost is real and is the reason this is a setting rather than always on: speech
//! during the sound is lost, not merely delayed. Someone wearing headphones has no loop
//! to break and should turn it off.
//!
//! The gate is a deadline, not a flag. Overlapping sounds extend it, nothing has to
//! remember to release it, and a crash between "start" and "stop" cannot leave the
//! microphone muted forever.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// An upper bound on a single suppression, so a very long file — or a wrong duration
/// read from a corrupt header — cannot mute the microphone for the rest of the session.
pub const MAX_SUPPRESSION: Duration = Duration::from_secs(30);

/// Shared "ignore the microphone until" deadline, in milliseconds since the Unix epoch.
///
/// Cloning shares the deadline; the playback side and the capture side each hold one.
#[derive(Clone, Default)]
pub struct PlaybackGate {
    /// Zero means "never suppressed", which is what a gate starts as.
    until_ms: Arc<AtomicU64>,
}

impl PlaybackGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ignore the microphone for `duration` from now.
    ///
    /// Only ever moves the deadline later: a short sound starting on top of a long one
    /// must not un-mute the microphone while the long one is still playing.
    pub fn suppress_for(&self, duration: Duration) {
        let duration = duration.min(MAX_SUPPRESSION);
        let deadline = now_ms().saturating_add(duration.as_millis() as u64);
        self.until_ms.fetch_max(deadline, Ordering::Relaxed);
    }

    pub fn is_suppressed(&self) -> bool {
        self.until_ms.load(Ordering::Relaxed) > now_ms()
    }

    /// Un-mute immediately, e.g. when playback is stopped by hand.
    pub fn release(&self) {
        self.until_ms.store(0, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for PlaybackGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaybackGate")
            .field("suppressed", &self.is_suppressed())
            .finish()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_gate_lets_the_microphone_through() {
        assert!(!PlaybackGate::new().is_suppressed());
    }

    #[test]
    fn suppression_lasts_for_the_requested_duration() {
        let gate = PlaybackGate::new();
        gate.suppress_for(Duration::from_millis(500));
        assert!(gate.is_suppressed());
    }

    #[test]
    fn a_zero_duration_suppresses_nothing() {
        let gate = PlaybackGate::new();
        gate.suppress_for(Duration::ZERO);
        assert!(!gate.is_suppressed());
    }

    #[test]
    fn clones_share_one_deadline() {
        let playback_side = PlaybackGate::new();
        let capture_side = playback_side.clone();

        playback_side.suppress_for(Duration::from_secs(2));
        assert!(capture_side.is_suppressed());

        capture_side.release();
        assert!(!playback_side.is_suppressed());
    }

    /// A short sound layered on a long one must not cut the long one's suppression
    /// short. Only the later deadline wins.
    #[test]
    fn a_shorter_overlapping_sound_does_not_shorten_suppression() {
        let gate = PlaybackGate::new();
        gate.suppress_for(Duration::from_secs(10));
        let long = gate.until_ms.load(Ordering::Relaxed);

        gate.suppress_for(Duration::from_millis(100));
        assert_eq!(gate.until_ms.load(Ordering::Relaxed), long);
    }

    #[test]
    fn a_wrong_duration_cannot_mute_the_microphone_indefinitely() {
        let gate = PlaybackGate::new();
        gate.suppress_for(Duration::from_secs(60 * 60));

        let remaining = gate.until_ms.load(Ordering::Relaxed) - now_ms();
        assert!(
            remaining <= MAX_SUPPRESSION.as_millis() as u64,
            "{remaining} ms of suppression exceeds the cap"
        );
    }
}
