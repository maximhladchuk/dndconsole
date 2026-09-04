use serde::Serialize;

/// Error returned to the frontend.
///
/// Failures are explained rather than silently swallowed, so every command error
/// carries a stable `kind` the UI can branch on plus a message written for a human.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub kind: String,
    pub message: String,
}

impl CommandError {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for CommandError {}

/// What to tell someone whose microphone the operating system will not hand over.
///
/// The instruction is different on every platform, and the wrong one is worse than none:
/// telling a Windows user to open System Settings › Privacy & Security sends them looking
/// for a menu that does not exist.
pub fn microphone_permission_message() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macOS не дав програмі доступ до мікрофона. Відкрий Системні параметри › \
         Конфіденційність і безпека › Мікрофон і увімкни dndsound."
    }
    #[cfg(target_os = "windows")]
    {
        "Windows не дала програмі доступ до мікрофона. Відкрий Параметри › \
         Конфіденційність і захист › Мікрофон, увімкни доступ для застосунків і для \
         dndsound."
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "Операційна система не дала програмі доступ до мікрофона. Дозволь його в \
         налаштуваннях приватності."
    }
}

impl From<dndsound_store::Error> for CommandError {
    fn from(err: dndsound_store::Error) -> Self {
        use dndsound_store::Error as E;
        let kind = match err {
            E::Migration(_) => "migrationFailed",
            E::NotFound(_) => "notFound",
            E::Io(_) => "io",
            E::Sqlite(_) => "database",
            E::Decode { .. } | E::Encode(_) => "serialization",
        };
        // The full error chain goes to the log; the UI gets the readable summary.
        tracing::error!(error = %err, "store error");
        CommandError::new(kind, err.to_string())
    }
}
