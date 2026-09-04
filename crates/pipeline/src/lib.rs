//! The live audio pipeline: microphone capture, resampling, and (from Phase 4)
//! voice activity detection and speech recognition.
//!
//! Everything here runs off the UI thread. The realtime audio callback does the
//! absolute minimum — convert and hand off — and all real work happens on worker
//! threads, so a slow frame in React can never stall the microphone.

mod capture;
mod devices;
mod error;
mod gate;
mod level;
mod offline;
mod resample;
mod session;
mod stt;
mod vad;

pub use capture::{Capture, CaptureStatus};
pub use devices::{list_input_devices, InputDevice};
pub use error::{Error, Result};
pub use gate::{PlaybackGate, MAX_SUPPRESSION};
pub use level::Level;
pub use offline::{read_wav_16k_mono, run_file, run_samples, OfflineRun, OfflineSegment};
pub use resample::{downmix_to_mono, MonoResampler, TARGET_SAMPLE_RATE};
pub use session::{ListenSession, SessionConfig, SessionEvent, SessionModels};
pub use stt::{SpeechRecognizer, SttConfig, Transcript};
pub use vad::{Segment, Segmenter, SileroVad, VadConfig, VadEvent, FRAME_SAMPLES};
