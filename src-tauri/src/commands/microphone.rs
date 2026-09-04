//! Microphone commands: which devices exist, and starting or stopping listening.

use dndsound_pipeline::{list_input_devices, InputDevice};
use tauri::State;

use crate::capture::CaptureSnapshot;
use crate::commands::app::Res;
use crate::error::CommandError;
use crate::state::AppState;

#[tauri::command]
pub fn list_microphones() -> Res<Vec<InputDevice>> {
    list_input_devices().map_err(|e| {
        tracing::error!(error = %e, "could not list microphones");
        CommandError::new("microphoneFailed", e.to_string())
    })
}

/// Start listening on the microphone chosen in settings, or the system default.
#[tauri::command]
pub fn start_listening(state: State<'_, AppState>) -> Res<CaptureSnapshot> {
    let device = state.with_db(|db| Ok::<_, CommandError>(db.settings().load()?.input_device))?;
    state.capture().start(device.as_deref())
}

#[tauri::command]
pub fn stop_listening(state: State<'_, AppState>) -> Res<CaptureSnapshot> {
    Ok(state.capture().stop())
}

/// Current microphone state, including the level for the meter.
///
/// Polled by the UI while listening. It is deliberately a poll rather than a stream of
/// events: a level meter that misses a frame is fine, and an event per audio block
/// would be thousands of IPC messages a minute for no benefit.
#[tauri::command]
pub fn capture_status(state: State<'_, AppState>) -> Res<CaptureSnapshot> {
    Ok(state.capture().snapshot())
}
