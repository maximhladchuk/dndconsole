//! Session commands: listening, simulating, and inspecting what the detector thinks.

use dndsound_detect::{Decision, Detection};
use dndsound_pipeline::{SessionConfig, SessionModels, SttConfig, VadConfig};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::app::Res;
use crate::error::CommandError;
use crate::session::SessionSnapshot;
use crate::state::AppState;

/// Whisper model used for the in-flight partial transcripts.
///
/// Measured at roughly a quarter of turbo's decode time, which is what makes triggering
/// mid-sentence possible: ~190 ms against turbo's ~840 ms, measured.
const FAST_MODEL_ID: &str = "small-q5_1";
const VAD_MODEL_ID: &str = "silero-vad-16k";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationResult {
    pub detection: Detection,
    pub decision: Decision,
    /// Sounds that actually played, when the simulation was asked to play them.
    pub played: Vec<String>,
}

/// Start listening: microphone, voice activity detection, speech recognition, and the
/// loop that turns transcripts into sounds.
#[tauri::command]
pub fn start_session(app: AppHandle, state: State<'_, AppState>) -> Res<SessionSnapshot> {
    // The microphone level meter and the session both open the input device, so the
    // standalone capture is stopped first rather than competing for it.
    state.capture().stop();

    let settings = state.with_db(|db| Ok::<_, CommandError>(db.settings().load()?))?;

    let vad_model_path = state.models().require(VAD_MODEL_ID).map_err(|_| {
        CommandError::new(
            "modelMissing",
            "Модель виявлення голосу ще не завантажена. \
             Завантаж її на вкладці «Налаштування», перш ніж починати сесію.",
        )
    })?;

    let speech_model_path = state
        .models()
        .require(&settings.speech_model)
        .map_err(|_| {
            CommandError::new(
                "modelMissing",
                format!(
                    "Модель розпізнавання «{}» ще не завантажена.",
                    settings.speech_model
                ),
            )
        })?;

    // Optional: without it, partial transcripts are skipped and events fire only when a
    // sentence ends. That is a degradation, not a failure.
    let fast_model_path = state.models().require(FAST_MODEL_ID).ok();
    if fast_model_path.is_none() {
        tracing::warn!(
            "the fast model is not downloaded; events will only fire at the end of a sentence"
        );
    }

    let config = SessionConfig {
        device_name: settings.input_device.clone(),
        vad: VadConfig {
            speech_threshold: settings.vad_speech_threshold,
            min_speech_ms: settings.vad_min_speech_ms,
            silence_timeout_ms: settings.vad_silence_timeout_ms,
            pre_roll_ms: settings.vad_pre_roll_ms,
            post_roll_ms: settings.vad_post_roll_ms,
            max_segment_ms: settings.vad_max_segment_ms,
        },
        // The vocabulary prompt is chosen with the language rather than beside it: a
        // Ukrainian prompt in front of English audio wrecks the transcript.
        stt: SttConfig::for_language(match settings.language.as_str() {
            "auto" => None,
            other => Some(other),
        }),
        partials_enabled: fast_model_path.is_some(),
        ..SessionConfig::default()
    };

    state
        .detection()
        .set_sensitivity(settings.event_sensitivity);

    state.session().start(
        app,
        state.db_handle(),
        state.audio_handle(),
        state.detection_handle(),
        config,
        SessionModels {
            vad_model_path,
            speech_model_path,
            fast_model_path,
        },
        settings.suppress_mic_during_playback,
    )
}

#[tauri::command]
pub fn stop_session(state: State<'_, AppState>) -> Res<SessionSnapshot> {
    Ok(state.session().stop())
}

#[tauri::command]
pub fn session_status(state: State<'_, AppState>) -> Res<SessionSnapshot> {
    Ok(state.session().snapshot(state.detection().event_count()))
}

/// Text Simulation Mode: run typed narration through the exact detection path the
/// microphone uses.
///
/// This is the fastest way to tune events, and it is free by construction — the detector
/// is a pure function, so nothing needs to be mocked.
#[tauri::command]
pub fn simulate_transcript(
    state: State<'_, AppState>,
    text: String,
    play: bool,
) -> Res<SimulationResult> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(CommandError::new(
            "invalidInput",
            "Напиши текст оповіді для перевірки.",
        ));
    }

    let detection = state
        .detection()
        .detect(&text, true, dndsound_store::now_ms());
    let decision = state.detection().decide_deterministically(&detection);

    let mut played = Vec::new();
    if play {
        let db = state.db_handle();
        for trigger in &decision.triggers {
            match crate::detection::play_trigger(&db, state.audio(), &trigger.event_id) {
                Ok(Some(sound)) => played.push(sound.display_name),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(event = %trigger.event_id, error = %e, "simulation could not play")
                }
            }
        }
    }

    Ok(SimulationResult {
        detection,
        decision,
        played,
    })
}

/// One transcribed segment of a recorded file, with what the detector made of it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedSegment {
    pub segment: dndsound_pipeline::OfflineSegment,
    pub detection: Detection,
    pub decision: Decision,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedRun {
    pub segments: Vec<RecordedSegment>,
    pub audio_ms: u32,
    pub elapsed_ms: u32,
}

/// Recorded-audio test mode: run a WAV through the real pipeline.
///
/// The only difference from a live session is where the audio comes from. This is how a
/// real recording of a real game gets turned into a regression test, and how a session
/// that misfired can be reproduced without asking anyone to perform it again.
#[tauri::command]
pub fn run_recorded_audio(state: State<'_, AppState>, path: String) -> Res<RecordedRun> {
    if state.session().is_running() {
        return Err(CommandError::new(
            "sessionRunning",
            "Зупини сесію прослуховування, перш ніж проганяти запис через конвеєр.",
        ));
    }

    let settings = state.with_db(|db| Ok::<_, CommandError>(db.settings().load()?))?;

    let vad_model = state.models().require(VAD_MODEL_ID).map_err(|_| {
        CommandError::new("modelMissing", "Модель виявлення голосу не завантажена.")
    })?;
    let speech_model = state
        .models()
        .require(&settings.speech_model)
        .map_err(|_| {
            CommandError::new(
                "modelMissing",
                format!(
                    "Модель розпізнавання «{}» не завантажена.",
                    settings.speech_model
                ),
            )
        })?;

    let run = dndsound_pipeline::run_file(
        &path,
        vad_model,
        speech_model,
        VadConfig {
            speech_threshold: settings.vad_speech_threshold,
            min_speech_ms: settings.vad_min_speech_ms,
            silence_timeout_ms: settings.vad_silence_timeout_ms,
            pre_roll_ms: settings.vad_pre_roll_ms,
            post_roll_ms: settings.vad_post_roll_ms,
            max_segment_ms: settings.vad_max_segment_ms,
        },
        SttConfig::for_language(match settings.language.as_str() {
            "auto" => None,
            other => Some(other),
        }),
    )
    .map_err(|e| CommandError::new("recordedRunFailed", e.to_string()))?;

    // A recording is a fresh timeline, so cooldowns from a live session do not apply.
    state.detection().reset_history();

    let segments = run
        .segments
        .into_iter()
        .map(|segment| {
            let detection =
                state
                    .detection()
                    .detect(&segment.text, true, i64::from(segment.start_ms));
            let decision = state.detection().decide(&detection);
            RecordedSegment {
                segment,
                detection,
                decision,
            }
        })
        .collect();

    Ok(RecordedRun {
        segments,
        audio_ms: run.audio_ms,
        elapsed_ms: run.elapsed_ms,
    })
}

/// Clear cooldowns and duplicate history, so a simulation run starts clean.
#[tauri::command]
pub fn reset_detection_history(state: State<'_, AppState>) -> Res<()> {
    state.detection().reset_history();
    Ok(())
}
