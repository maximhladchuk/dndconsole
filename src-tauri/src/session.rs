//! The listening session, from the application's side.
//!
//! Owns the pipeline session and the loop that turns its transcripts into sounds:
//!
//! ```text
//! transcript -> detect -> decide (cooldown, duplicates, probability) -> play
//!                     \-> emit to the UI, including every rejected candidate
//! ```
//!
//! Everything the UI shows about a session comes from the events emitted here, so Debug
//! Mode never needs a second source of truth.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dndsound_pipeline::{ListenSession, PlaybackGate, SessionConfig, SessionEvent, SessionModels};
use dndsound_store::Db;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::audio::AudioState;
use crate::detection::{self, DetectionState};
use crate::error::CommandError;

/// Tauri event name the frontend listens on.
pub const SESSION_EVENT: &str = "session://event";

/// How often the worker checks for new transcripts.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Extra microphone suppression after a sound's own length has elapsed.
///
/// A sound does not stop being audible when the file ends: a room reverberates, and the
/// output device buffers a little ahead of what has been heard. Both are short; this is
/// deliberately generous because the cost of being wrong in the other direction is a
/// sound triggering itself.
const SUPPRESSION_TAIL: Duration = Duration::from_millis(400);

/// How long to mute for when a sound's duration is unknown — an unreadable header, or a
/// format symphonia could not count frames for. Long enough for a typical effect.
const SUPPRESSION_WITHOUT_DURATION: Duration = Duration::from_millis(2_000);

/// How long the microphone must ignore `sound`.
fn suppression_for(duration_ms: Option<i64>) -> Duration {
    match duration_ms {
        Some(ms) if ms > 0 => Duration::from_millis(ms as u64) + SUPPRESSION_TAIL,
        _ => SUPPRESSION_WITHOUT_DURATION,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum SessionUpdate {
    SpeechStarted {
        at_ms: i64,
    },
    Transcript {
        text: String,
        is_final: bool,
        at_ms: i64,
        stt_ms: u32,
        speech_ms: u32,
        language: Option<String>,
    },
    /// The full detection result, including rejected candidates and why.
    Detection {
        detection: dndsound_detect::Detection,
        decision: dndsound_detect::Decision,
        detect_us: u64,
    },
    /// A sound actually started playing.
    Played {
        event_id: String,
        sound_name: String,
        confidence: f32,
        at_ms: i64,
        /// From the end of the transcript to the sound starting.
        latency_ms: u32,
    },
    /// An event fired but had nothing to play.
    NoSound {
        event_id: String,
        reason: String,
    },
    /// Speech that produced no usable transcript.
    Discarded {
        at_ms: i64,
        speech_ms: u32,
        reason: String,
    },
    Error {
        message: String,
    },
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub running: bool,
    pub device_name: Option<String>,
    pub level: f32,
    pub event_count: usize,
    pub started_at_ms: Option<i64>,
}

struct Running {
    session: Arc<Mutex<Option<ListenSession>>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    device_name: String,
    started_at_ms: i64,
}

#[derive(Default)]
pub struct SessionState {
    running: Mutex<Option<Running>>,
    /// Whether playback mutes the microphone. Lives here rather than being read from
    /// the database per sound so that toggling the setting takes effect immediately,
    /// without restarting the session.
    suppress_playback: Arc<AtomicBool>,
}

impl SessionState {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        app: AppHandle,
        db: Arc<Mutex<Db>>,
        audio: Arc<AudioState>,
        detection: Arc<DetectionState>,
        config: SessionConfig,
        models: SessionModels,
        suppress_playback: bool,
    ) -> Result<SessionSnapshot, CommandError> {
        self.set_suppress_playback(suppress_playback);
        let mut guard = self.lock();
        if guard.is_some() {
            return Err(CommandError::new(
                "sessionRunning",
                "A listening session is already running.",
            ));
        }

        detection.reset_history();

        let session = ListenSession::start(config, models).map_err(session_error)?;
        let device_name = session.device_name().to_string();
        let gate = session.gate();
        tracing::info!(device = %device_name, "listening session started");

        let session = Arc::new(Mutex::new(Some(session)));
        let stop = Arc::new(AtomicBool::new(false));
        let started_at_ms = dndsound_store::now_ms();

        let worker = std::thread::Builder::new()
            .name("dndsound-session-events".to_string())
            .spawn({
                let session = Arc::clone(&session);
                let stop = Arc::clone(&stop);
                let suppress = Arc::clone(&self.suppress_playback);
                move || run(app, db, audio, detection, session, gate, suppress, stop)
            })
            .map_err(|e| CommandError::new("sessionFailed", e.to_string()))?;

        *guard = Some(Running {
            session,
            stop,
            worker: Some(worker),
            device_name: device_name.clone(),
            started_at_ms,
        });

        Ok(SessionSnapshot {
            running: true,
            device_name: Some(device_name),
            level: 0.0,
            event_count: 0,
            started_at_ms: Some(started_at_ms),
        })
    }

    pub fn stop(&self) -> SessionSnapshot {
        let mut guard = self.lock();

        if let Some(mut running) = guard.take() {
            running.stop.store(true, Ordering::Relaxed);
            if let Some(worker) = running.worker.take() {
                let _ = worker.join();
            }
            if let Ok(mut session) = running.session.lock() {
                if let Some(mut session) = session.take() {
                    session.stop();
                }
            }
            tracing::info!("listening session stopped");
        }

        SessionSnapshot {
            running: false,
            device_name: None,
            level: 0.0,
            event_count: 0,
            started_at_ms: None,
        }
    }

    pub fn snapshot(&self, event_count: usize) -> SessionSnapshot {
        let guard = self.lock();

        match guard.as_ref() {
            Some(running) => {
                let level = running
                    .session
                    .lock()
                    .ok()
                    .and_then(|session| session.as_ref().map(|s| s.level()))
                    .unwrap_or(0.0);

                SessionSnapshot {
                    running: true,
                    device_name: Some(running.device_name.clone()),
                    level,
                    event_count,
                    started_at_ms: Some(running.started_at_ms),
                }
            }
            None => SessionSnapshot {
                running: false,
                device_name: None,
                level: 0.0,
                event_count,
                started_at_ms: None,
            },
        }
    }

    pub fn is_running(&self) -> bool {
        self.lock().is_some()
    }

    /// Turn microphone suppression during playback on or off, mid-session included.
    pub fn set_suppress_playback(&self, enabled: bool) {
        self.suppress_playback.store(enabled, Ordering::Relaxed);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Running>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A trigger waiting for its delay to elapse, from an event chain.
struct Scheduled {
    event_id: String,
    confidence: f32,
    due: Instant,
    transcript_ended: Instant,
}

#[allow(clippy::too_many_arguments)]
fn run(
    app: AppHandle,
    db: Arc<Mutex<Db>>,
    audio: Arc<AudioState>,
    detection: Arc<DetectionState>,
    session: Arc<Mutex<Option<ListenSession>>>,
    gate: PlaybackGate,
    suppress: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    let mut scheduled: Vec<Scheduled> = Vec::new();

    while !stop.load(Ordering::Relaxed) {
        let events = {
            let guard = session.lock();
            match guard {
                Ok(guard) => guard.as_ref().map(|s| s.drain()).unwrap_or_default(),
                Err(_) => break,
            }
        };

        for event in events {
            handle(
                &app,
                &db,
                &audio,
                &detection,
                &gate,
                &suppress,
                event,
                &mut scheduled,
            );
        }

        // Fire anything whose delay has come due.
        let now = Instant::now();
        let (due, waiting): (Vec<Scheduled>, Vec<Scheduled>) =
            scheduled.into_iter().partition(|item| item.due <= now);
        scheduled = waiting;

        for item in due {
            play(
                &app,
                &db,
                &audio,
                &gate,
                &suppress,
                &item.event_id,
                item.confidence,
                item.transcript_ended,
            );
        }

        std::thread::sleep(POLL_INTERVAL);
    }

    let _ = app.emit(SESSION_EVENT, SessionUpdate::Stopped);
}

#[allow(clippy::too_many_arguments)]
fn handle(
    app: &AppHandle,
    db: &Arc<Mutex<Db>>,
    audio: &Arc<AudioState>,
    detection: &Arc<DetectionState>,
    gate: &PlaybackGate,
    suppress: &Arc<AtomicBool>,
    event: SessionEvent,
    scheduled: &mut Vec<Scheduled>,
) {
    match event {
        SessionEvent::SpeechStarted { .. } => {
            let _ = app.emit(
                SESSION_EVENT,
                SessionUpdate::SpeechStarted {
                    at_ms: dndsound_store::now_ms(),
                },
            );
        }

        SessionEvent::Partial {
            text,
            stream_ms: _,
            stt_ms,
            speech_ms,
        } => {
            // The pipeline reports a position in the audio stream. The interface shows a
            // time of day, so the wall clock is attached here rather than there — the UI
            // was rendering the stream offset as a date and showing 1970.
            let at_ms = dndsound_store::now_ms();
            let _ = app.emit(
                SESSION_EVENT,
                SessionUpdate::Transcript {
                    text: text.clone(),
                    is_final: false,
                    at_ms,
                    stt_ms,
                    speech_ms,
                    language: None,
                },
            );
            process(
                app, db, audio, detection, gate, suppress, &text, false, at_ms, scheduled,
            );
        }

        SessionEvent::Final {
            text,
            stream_ms: _,
            stt_ms,
            speech_ms,
            language,
            ..
        } => {
            let at_ms = dndsound_store::now_ms();
            let _ = app.emit(
                SESSION_EVENT,
                SessionUpdate::Transcript {
                    text: text.clone(),
                    is_final: true,
                    at_ms,
                    stt_ms,
                    speech_ms,
                    language,
                },
            );
            process(
                app, db, audio, detection, gate, suppress, &text, true, at_ms, scheduled,
            );
        }

        SessionEvent::Discarded {
            stream_ms: _,
            speech_ms,
            reason,
        } => {
            let at_ms = dndsound_store::now_ms();
            let _ = app.emit(
                SESSION_EVENT,
                SessionUpdate::Discarded {
                    at_ms,
                    speech_ms,
                    reason,
                },
            );
        }

        SessionEvent::Error { message } => {
            tracing::error!(message, "session error");
            let _ = app.emit(SESSION_EVENT, SessionUpdate::Error { message });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn process(
    app: &AppHandle,
    db: &Arc<Mutex<Db>>,
    audio: &Arc<AudioState>,
    detection: &Arc<DetectionState>,
    gate: &PlaybackGate,
    suppress: &Arc<AtomicBool>,
    text: &str,
    is_final: bool,
    at_ms: i64,
    scheduled: &mut Vec<Scheduled>,
) {
    // The moment the transcript was ready is the reference point for latency: everything
    // after it is time the Dungeon Master spends waiting for a sound.
    let transcript_ended = Instant::now();

    let result = detection.detect(text, is_final, at_ms);
    let decision = detection.decide(&result);

    let _ = app.emit(
        SESSION_EVENT,
        SessionUpdate::Detection {
            detect_us: result.elapsed_us,
            detection: result,
            decision: decision.clone(),
        },
    );

    for trigger in decision.triggers {
        if trigger.delay_ms == 0 {
            play(
                app,
                db,
                audio,
                gate,
                suppress,
                &trigger.event_id,
                trigger.confidence,
                transcript_ended,
            );
        } else {
            scheduled.push(Scheduled {
                event_id: trigger.event_id,
                confidence: trigger.confidence,
                due: Instant::now() + Duration::from_millis(u64::from(trigger.delay_ms)),
                transcript_ended,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn play(
    app: &AppHandle,
    db: &Arc<Mutex<Db>>,
    audio: &Arc<AudioState>,
    gate: &PlaybackGate,
    suppress: &Arc<AtomicBool>,
    event_id: &str,
    confidence: f32,
    transcript_ended: Instant,
) {
    match detection::play_trigger(db, audio, event_id) {
        Ok(Some(sound)) => {
            // Before anything else: the speakers are now emitting something the
            // microphone will hear, and it must not become the next transcript.
            if suppress.load(Ordering::Relaxed) {
                gate.suppress_for(suppression_for(sound.duration_ms));
            }

            let latency_ms = transcript_ended.elapsed().as_millis() as u32;
            tracing::info!(event = event_id, sound = %sound.display_name, latency_ms, "played");

            let _ = app.emit(
                SESSION_EVENT,
                SessionUpdate::Played {
                    event_id: event_id.to_string(),
                    sound_name: sound.display_name,
                    confidence,
                    at_ms: dndsound_store::now_ms(),
                    latency_ms,
                },
            );
        }
        Ok(None) => {
            let _ = app.emit(
                SESSION_EVENT,
                SessionUpdate::NoSound {
                    event_id: event_id.to_string(),
                    reason: "no sound group is assigned to this event".to_string(),
                },
            );
        }
        Err(e) => {
            let _ = app.emit(
                SESSION_EVENT,
                SessionUpdate::NoSound {
                    event_id: event_id.to_string(),
                    reason: e.message,
                },
            );
        }
    }
}

fn session_error(err: dndsound_pipeline::Error) -> CommandError {
    use dndsound_pipeline::Error as E;
    let kind = match err {
        E::PermissionDenied => "microphonePermissionDenied",
        E::NoInputDevice => "noMicrophone",
        E::DeviceNotFound(_) => "microphoneNotFound",
        E::Vad(_) => "vadFailed",
        E::Stt(_) => "sttFailed",
        _ => "sessionFailed",
    };

    let message = match &err {
        E::PermissionDenied => "macOS has not granted this app access to the microphone. \
             Open System Settings › Privacy & Security › Microphone and enable dndsound."
            .to_string(),
        other => other.to_string(),
    };

    tracing::error!(error = %err, "session failed to start");
    CommandError::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Every key anywhere in the value, however deeply nested.
    fn keys(value: &Value, into: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    into.push(key.clone());
                    keys(child, into);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| keys(item, into)),
            _ => {}
        }
    }

    #[track_caller]
    fn assert_camel_case(update: &SessionUpdate) {
        let json = serde_json::to_value(update).expect("serialize");
        let mut found = Vec::new();
        keys(&json, &mut found);

        for key in found {
            assert!(
                !key.contains('_'),
                "'{key}' is snake_case; src/types/api.ts expects camelCase. Full value: {json}"
            );
        }
    }

    /// `rename_all` on an enum renames the variants only. Without `rename_all_fields`,
    /// `at_ms` and `stt_ms` reach the UI under names it does not read, and nothing on
    /// either side reports an error — the transcript panel just shows "Invalid Date".
    #[test]
    fn every_session_update_reaches_the_ui_in_camel_case() {
        let updates = vec![
            SessionUpdate::SpeechStarted {
                at_ms: 1_700_000_000_000,
            },
            SessionUpdate::Transcript {
                text: "he swings his sword".to_string(),
                is_final: true,
                at_ms: 1_700_000_000_000,
                stt_ms: 740,
                speech_ms: 1_900,
                language: Some("uk".to_string()),
            },
            SessionUpdate::Played {
                event_id: "SWORD_SWING".to_string(),
                sound_name: "sword swing 01".to_string(),
                confidence: 0.91,
                at_ms: 1_700_000_000_000,
                latency_ms: 820,
            },
            SessionUpdate::NoSound {
                event_id: "THUNDER".to_string(),
                reason: "the group is empty".to_string(),
            },
            SessionUpdate::Discarded {
                at_ms: 1_700_000_000_000,
                speech_ms: 300,
                reason: "no words".to_string(),
            },
            SessionUpdate::Error {
                message: "device lost".to_string(),
            },
            SessionUpdate::Stopped,
        ];

        for update in &updates {
            assert_camel_case(update);
        }
    }

    #[test]
    fn a_transcript_matches_the_typescript_mirror_field_for_field() {
        let json = serde_json::to_value(SessionUpdate::Transcript {
            text: "відчиняє двері".to_string(),
            is_final: false,
            at_ms: 1_700_000_000_000_i64,
            stt_ms: 210,
            speech_ms: 900,
            language: Some("uk".to_string()),
        })
        .expect("serialize");

        assert_eq!(json["kind"], "transcript");
        assert_eq!(json["isFinal"], false);
        assert_eq!(json["atMs"], 1_700_000_000_000_i64);
        assert_eq!(json["sttMs"], 210);
        assert_eq!(json["speechMs"], 900);
        assert_eq!(json["language"], "uk");
    }

    #[test]
    fn a_sound_is_muted_for_its_own_length_plus_a_tail() {
        let suppression = suppression_for(Some(1_500));
        assert_eq!(suppression, Duration::from_millis(1_500) + SUPPRESSION_TAIL);
    }

    /// A file whose header could not be read still has to mute the microphone. Treating
    /// an unknown duration as zero would leave exactly the loop this exists to break.
    #[test]
    fn an_unknown_duration_still_mutes_the_microphone() {
        for unknown in [None, Some(0), Some(-1)] {
            assert_eq!(suppression_for(unknown), SUPPRESSION_WITHOUT_DURATION);
            assert!(suppression_for(unknown) > Duration::ZERO);
        }
    }

    #[test]
    fn the_snapshot_reaches_the_ui_in_camel_case() {
        let json = serde_json::to_value(SessionSnapshot {
            running: true,
            device_name: Some("MacBook Pro Microphone".to_string()),
            level: 0.2,
            event_count: 5,
            started_at_ms: Some(1_700_000_000_000),
        })
        .expect("serialize");

        assert_eq!(json["deviceName"], "MacBook Pro Microphone");
        assert_eq!(json["eventCount"], 5);
        assert_eq!(json["startedAtMs"], 1_700_000_000_000_i64);
    }
}
