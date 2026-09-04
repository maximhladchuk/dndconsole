//! The listening session: microphone in, transcripts out.
//!
//! This is where capture, voice activity detection and speech recognition are stitched
//! together, on a worker thread that owns them all. It deliberately stops at
//! transcripts — event detection and playback live above it, so that a wrong sound can
//! never be blamed on the audio layer and vice versa.
//!
//! Two decode paths run over the same speech:
//!
//! * **Partial** — while the sentence is still being spoken, the audio so far is
//!   re-transcribed every few hundred milliseconds with the fast model. This is what
//!   lets "…pulls out its *sword*" fire before the sentence ends.
//! * **Final** — when the segment closes, it is transcribed once with the accurate
//!   model. This is the transcript that goes in the log.
//!
//! Measurements behind the two-model split are in `docs/PERFORMANCE.md`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::capture::Capture;
use crate::gate::PlaybackGate;
use crate::stt::{SpeechRecognizer, SttConfig};
use crate::vad::{Segmenter, SileroVad, VadConfig, VadEvent, FRAME_SAMPLES};
use crate::{Error, Result, TARGET_SAMPLE_RATE};

/// How often the in-flight sentence is re-transcribed.
///
/// Fast enough to feel responsive, slow enough that the fast model keeps up: on the
/// reference machine a partial decode takes ~200 ms, so 400 ms leaves headroom.
const DEFAULT_PARTIAL_INTERVAL_MS: u64 = 400;

/// Do not bother transcribing less than this much speech.
const MIN_PARTIAL_MS: u64 = 500;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum SessionEvent {
    SpeechStarted {
        /// Milliseconds since the session started — a position in the audio stream, not
        /// a wall clock. Rendering it as a time of day produces 1970 plus a few minutes,
        /// which is exactly what the transcript panel used to show.
        stream_ms: i64,
    },
    /// A transcript of a sentence still being spoken.
    Partial {
        text: String,
        stream_ms: i64,
        stt_ms: u32,
        speech_ms: u32,
    },
    /// The settled transcript of a finished sentence.
    Final {
        text: String,
        stream_ms: i64,
        stt_ms: u32,
        speech_ms: u32,
        language: Option<String>,
        truncated: bool,
    },
    /// Speech ended but produced nothing believable — silence, noise, or a hallucination.
    Discarded {
        stream_ms: i64,
        speech_ms: u32,
        reason: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    pub device_name: Option<String>,
    pub vad: VadConfig,
    pub stt: SttConfig,
    pub partial_interval_ms: u64,
    /// Run partial decodes at all. Turning this off halves the work at the cost of
    /// firing only once a sentence has ended.
    pub partials_enabled: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            vad: VadConfig::default(),
            stt: SttConfig::default(),
            partial_interval_ms: DEFAULT_PARTIAL_INTERVAL_MS,
            partials_enabled: true,
        }
    }
}

/// The models a session needs, loaded before it starts.
pub struct SessionModels {
    pub vad_model_path: std::path::PathBuf,
    /// The accurate model, used for the final transcript.
    pub speech_model_path: std::path::PathBuf,
    /// The fast model for partial re-decodes. Falls back to the accurate one when absent.
    pub fast_model_path: Option<std::path::PathBuf>,
}

pub struct ListenSession {
    capture: Arc<Mutex<Option<Capture>>>,
    gate: PlaybackGate,
    stop: Arc<AtomicBool>,
    events: Receiver<SessionEvent>,
    worker: Option<std::thread::JoinHandle<()>>,
    device_name: String,
}

impl ListenSession {
    /// Start listening. Models are loaded before the microphone opens, so a missing
    /// model fails immediately rather than after the first sentence.
    pub fn start(config: SessionConfig, models: SessionModels) -> Result<Self> {
        let mut vad = SileroVad::load(&models.vad_model_path)?;
        vad.reset();

        let accurate = SpeechRecognizer::load(&models.speech_model_path, config.stt.clone())?;
        let fast = match &models.fast_model_path {
            Some(path) => Some(SpeechRecognizer::load(path, config.stt.clone())?),
            None => None,
        };

        let capture = Capture::start(config.device_name.as_deref())?;
        let device_name = capture.device_name().to_string();
        let capture = Arc::new(Mutex::new(Some(capture)));

        let (sender, events) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let gate = PlaybackGate::new();

        let worker = std::thread::Builder::new()
            .name("dndsound-session".to_string())
            .spawn({
                let capture = Arc::clone(&capture);
                let stop = Arc::clone(&stop);
                let gate = gate.clone();
                move || {
                    run(config, vad, accurate, fast, capture, gate, stop, sender);
                }
            })
            .map_err(|e| Error::Open(e.to_string()))?;

        Ok(Self {
            capture,
            gate,
            stop,
            events,
            worker: Some(worker),
            device_name,
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// The handle playback uses to mute the microphone while a sound is audible.
    ///
    /// A session always has one. Whether anything ever calls `suppress_for` on it is
    /// the caller's decision, which is how the setting stays out of the pipeline.
    pub fn gate(&self) -> PlaybackGate {
        self.gate.clone()
    }

    /// Everything that has happened since the last call. Never blocks.
    pub fn drain(&self) -> Vec<SessionEvent> {
        self.events.try_iter().collect()
    }

    /// Smoothed input level, for the meter.
    pub fn level(&self) -> f32 {
        self.capture
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|capture| capture.level()))
            .unwrap_or(0.0)
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Ok(mut guard) = self.capture.lock() {
            if let Some(mut capture) = guard.take() {
                capture.stop();
            }
        }
    }
}

impl Drop for ListenSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    config: SessionConfig,
    mut vad: SileroVad,
    accurate: SpeechRecognizer,
    fast: Option<SpeechRecognizer>,
    capture: Arc<Mutex<Option<Capture>>>,
    gate: PlaybackGate,
    stop: Arc<AtomicBool>,
    sender: Sender<SessionEvent>,
) {
    let mut segmenter = Segmenter::new(config.vad);
    let mut pending: Vec<f32> = Vec::with_capacity(FRAME_SAMPLES * 4);
    let mut last_partial = Instant::now();
    let started = Instant::now();
    let mut muted = false;

    // The fast model drives partials; without one, partials use the accurate model,
    // which on the reference machine is too slow to keep up and is therefore skipped.
    let partial_recognizer = fast.as_ref();
    let partials_enabled = config.partials_enabled && partial_recognizer.is_some();

    while !stop.load(Ordering::Relaxed) {
        let samples = {
            let guard = capture.lock();
            match guard {
                Ok(guard) => guard.as_ref().map(|c| c.take_pcm()).unwrap_or_default(),
                Err(_) => break,
            }
        };

        // The application's own sounds reach the microphone. While one is audible the
        // input is thrown away rather than filtered: the sentence that overlaps a
        // thunderclap is lost, but the thunderclap cannot transcribe into words and
        // trigger a second one.
        if gate.is_suppressed() {
            if !muted {
                muted = true;
                // Whatever was half-said when the sound started is abandoned. Keeping
                // it would splice speech onto the far side of the gap and hand whisper
                // a sentence with a hole in it.
                pending.clear();
                segmenter.reset();
                vad.reset();
            }
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }

        if muted {
            muted = false;
            // The first frames after the gap are the tail of the room's reverberation.
            // The segmenter starts from silence so they cannot open a segment on their
            // own; a person still talking opens one on the next frame as usual.
            segmenter.reset();
            vad.reset();
        }

        if samples.is_empty() {
            std::thread::sleep(Duration::from_millis(10));
        } else {
            pending.extend_from_slice(&samples);
        }

        while pending.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();

            let probability = match vad.probability(&frame) {
                Ok(probability) => probability,
                Err(e) => {
                    let _ = sender.send(SessionEvent::Error {
                        message: e.to_string(),
                    });
                    return;
                }
            };

            let now_ms = started.elapsed().as_millis() as i64;

            match segmenter.push(&frame, probability) {
                VadEvent::SpeechStarted => {
                    last_partial = Instant::now();
                    let _ = sender.send(SessionEvent::SpeechStarted { stream_ms: now_ms });
                }

                VadEvent::Speaking { .. } if partials_enabled => {
                    let audio = segmenter.current_audio();
                    let speech_ms = duration_ms(audio.len());

                    if last_partial.elapsed().as_millis() as u64 >= config.partial_interval_ms
                        && speech_ms as u64 >= MIN_PARTIAL_MS
                    {
                        last_partial = Instant::now();
                        let recognizer = partial_recognizer.expect("checked above");

                        match recognizer.transcribe(audio, true) {
                            Ok(transcript) if recognizer.is_trustworthy(&transcript) => {
                                let _ = sender.send(SessionEvent::Partial {
                                    text: transcript.text,
                                    stream_ms: now_ms,
                                    stt_ms: transcript.elapsed_ms,
                                    speech_ms,
                                });
                            }
                            Ok(_) => {}
                            Err(e) => {
                                let _ = sender.send(SessionEvent::Error {
                                    message: e.to_string(),
                                });
                            }
                        }
                    }
                }

                VadEvent::SegmentReady(segment) => {
                    let speech_ms = segment.duration_ms();

                    match accurate.transcribe(&segment.samples, false) {
                        Ok(transcript) if accurate.is_trustworthy(&transcript) => {
                            let _ = sender.send(SessionEvent::Final {
                                text: transcript.text,
                                stream_ms: now_ms,
                                stt_ms: transcript.elapsed_ms,
                                speech_ms,
                                language: transcript.language,
                                truncated: segment.truncated,
                            });
                        }
                        Ok(transcript) => {
                            // Not an error: silence and noise routinely reach here, and
                            // saying so is what makes Debug Mode useful.
                            let _ = sender.send(SessionEvent::Discarded {
                                stream_ms: now_ms,
                                speech_ms,
                                reason: if transcript.text.trim().is_empty() {
                                    "no words were recognised".to_string()
                                } else {
                                    format!("transcript rejected: {:?}", transcript.text)
                                },
                            });
                        }
                        Err(e) => {
                            let _ = sender.send(SessionEvent::Error {
                                message: e.to_string(),
                            });
                        }
                    }
                }

                _ => {}
            }
        }
    }

    // Whatever was being said when listening stopped still deserves a transcript.
    if let Some(segment) = segmenter.flush() {
        if let Ok(transcript) = accurate.transcribe(&segment.samples, false) {
            if accurate.is_trustworthy(&transcript) {
                let _ = sender.send(SessionEvent::Final {
                    text: transcript.text,
                    stream_ms: started.elapsed().as_millis() as i64,
                    stt_ms: transcript.elapsed_ms,
                    speech_ms: segment.duration_ms(),
                    language: transcript.language,
                    truncated: segment.truncated,
                });
            }
        }
    }
}

fn duration_ms(samples: usize) -> u32 {
    (samples as f32 / TARGET_SAMPLE_RATE as f32 * 1000.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_counts_convert_to_milliseconds() {
        assert_eq!(duration_ms(TARGET_SAMPLE_RATE as usize), 1_000);
        assert_eq!(duration_ms(TARGET_SAMPLE_RATE as usize / 2), 500);
        assert_eq!(duration_ms(0), 0);
    }

    #[test]
    fn the_default_partial_interval_leaves_the_fast_model_headroom() {
        let config = SessionConfig::default();
        // A partial decode measured ~200 ms; the interval must exceed it or the queue
        // grows without bound.
        assert!(config.partial_interval_ms >= 300);
        assert!(config.partials_enabled);
    }

    #[test]
    fn session_events_serialize_for_the_ui() {
        let event = SessionEvent::Final {
            text: "You open the door".to_string(),
            stream_ms: 1_000,
            stt_ms: 700,
            speech_ms: 2_000,
            language: Some("en".to_string()),
            truncated: false,
        };

        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"type\":\"final\""), "got {json}");

        // The variant tag being camelCase is not enough. `rename_all` on an enum renames
        // the *variants* only, so the fields inside a variant stay snake_case unless
        // `rename_all_fields` says otherwise. That mismatch does not fail anywhere: the
        // UI simply reads `undefined` for every multi-word field and renders it as an
        // empty string or an Invalid Date.
        for key in ["\"streamMs\"", "\"sttMs\"", "\"speechMs\""] {
            assert!(json.contains(key), "{key} missing from {json}");
        }
        assert!(
            !json.contains("_ms"),
            "a snake_case field leaked into {json}"
        );
    }
}
