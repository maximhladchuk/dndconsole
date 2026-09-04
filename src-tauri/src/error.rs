use serde::Serialize;

/// Error returned to the frontend.
///
/// The spec is explicit that failures must be explained rather than silently swallowed,
/// so every command error carries a stable `kind` the UI can branch on plus a message
/// written for a human.
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
