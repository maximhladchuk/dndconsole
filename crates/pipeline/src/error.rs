use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no microphone is available")]
    NoInputDevice,

    #[error("microphone '{0}' was not found; it may have been unplugged")]
    DeviceNotFound(String),

    #[error("could not read the list of microphones: {0}")]
    Enumeration(String),

    #[error("microphone '{device}' rejected the requested audio format: {reason}")]
    UnsupportedFormat { device: String, reason: String },

    #[error("could not open the microphone: {0}")]
    Open(String),

    #[error("macOS denied access to the microphone for this application")]
    PermissionDenied,

    #[error("the microphone stopped: {0}")]
    Stream(String),

    #[error("voice activity detection failed: {0}")]
    Vad(String),

    #[error("speech recognition failed: {0}")]
    Stt(String),

    #[error("could not set up resampling from {from} Hz to {to} Hz: {reason}")]
    Resampler { from: u32, to: u32, reason: String },
}
