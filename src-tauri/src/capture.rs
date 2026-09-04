//! Microphone state for the running application.

use std::sync::Mutex;

use dndsound_pipeline::{Capture, CaptureStatus, Error as PipelineError};
use serde::Serialize;

use crate::error::CommandError;

#[derive(Default)]
pub struct CaptureState {
    capture: Mutex<Option<Capture>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSnapshot {
    pub listening: bool,
    pub device_name: Option<String>,
    pub input_sample_rate: Option<u32>,
    pub input_channels: Option<u16>,
    /// Smoothed RMS in linear amplitude, for the level meter.
    pub level: f32,
    pub status: Option<CaptureStatus>,
}

impl CaptureState {
    pub fn start(&self, device_name: Option<&str>) -> Result<CaptureSnapshot, CommandError> {
        let mut guard = self.lock();

        // Restarting on the same device would otherwise leave two streams open.
        if let Some(mut existing) = guard.take() {
            existing.stop();
        }

        let capture = Capture::start(device_name).map_err(capture_error)?;
        tracing::info!(device = %capture.device_name(), "listening");
        *guard = Some(capture);

        Ok(snapshot_of(guard.as_ref()))
    }

    pub fn stop(&self) -> CaptureSnapshot {
        let mut guard = self.lock();
        if let Some(mut capture) = guard.take() {
            capture.stop();
        }
        snapshot_of(None)
    }

    pub fn snapshot(&self) -> CaptureSnapshot {
        let guard = self.lock();
        snapshot_of(guard.as_ref())
    }

    /// Take the audio captured since the last call, as 16 kHz mono.
    ///
    /// From Phase 4 this feeds voice activity detection; for now it backs the
    /// diagnostics in Debug Mode.
    pub fn take_pcm(&self) -> Vec<f32> {
        let guard = self.lock();
        guard.as_ref().map(|c| c.take_pcm()).unwrap_or_default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Capture>> {
        self.capture
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn snapshot_of(capture: Option<&Capture>) -> CaptureSnapshot {
    match capture {
        Some(capture) => {
            let status = capture.status();
            CaptureSnapshot {
                // A failed stream is not listening, whatever the handle says.
                listening: status == CaptureStatus::Running,
                device_name: Some(capture.device_name().to_string()),
                input_sample_rate: Some(capture.input_sample_rate()),
                input_channels: Some(capture.input_channels()),
                level: capture.level(),
                status: Some(status),
            }
        }
        None => CaptureSnapshot {
            listening: false,
            device_name: None,
            input_sample_rate: None,
            input_channels: None,
            level: 0.0,
            status: Some(CaptureStatus::Stopped),
        },
    }
}

/// Translate a capture failure into something the UI can act on.
fn capture_error(err: PipelineError) -> CommandError {
    use PipelineError as E;
    let kind = match err {
        E::PermissionDenied => "microphonePermissionDenied",
        E::NoInputDevice => "noMicrophone",
        E::DeviceNotFound(_) => "microphoneNotFound",
        E::UnsupportedFormat { .. } => "microphoneFormat",
        E::Enumeration(_) | E::Open(_) | E::Stream(_) => "microphoneFailed",
        E::Resampler { .. } => "resamplerFailed",
        // Reachable only through the session, which reports these itself; kept explicit
        // so a new pipeline error cannot be silently mapped to the wrong thing.
        E::Vad(_) => "vadFailed",
        E::Stt(_) => "sttFailed",
    };

    let message = match &err {
        E::PermissionDenied => crate::error::microphone_permission_message().to_string(),
        other => other.to_string(),
    };

    tracing::error!(error = %err, "capture error");
    CommandError::new(kind, message)
}
