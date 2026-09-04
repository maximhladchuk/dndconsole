//! Application state shared across commands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dndsound_models::ModelStore;
use dndsound_store::Db;
use tauri::{AppHandle, Manager};

use crate::audio::AudioState;
use crate::capture::CaptureState;
use crate::detection::DetectionState;
use crate::session::SessionState;

pub struct AppState {
    /// Behind a mutex rather than a pool: the store is synchronous, commands are short,
    /// and the session worker needs to share it. `Arc` so that worker can hold it.
    db: Arc<Mutex<Db>>,
    db_path: PathBuf,
    library_dir: PathBuf,
    audio: Arc<AudioState>,
    detection: Arc<DetectionState>,
    capture: CaptureState,
    session: SessionState,
    models: ModelStore,
}

impl AppState {
    pub fn initialize(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("dndsound.db");
        tracing::info!(path = %db_path.display(), "opening database");

        let db = Db::open(&db_path)?;
        db.profiles().ensure_default()?;
        // A fresh install ships with working events so the pipeline can be tried
        // immediately; deleted events are never resurrected.
        db.events().seed_if_empty()?;
        // And an existing install picks up improvements to those events. Without this,
        // a fix to the built-in phrasing only ever reached people who had never run the
        // application before. Events a person has edited are left alone.
        db.events().sync_builtin()?;

        let settings = db.settings().load()?;
        let audio = Arc::new(AudioState::initialize(&settings));
        let detection = Arc::new(DetectionState::load(&db)?);
        let models = ModelStore::new(data_dir.join("models"));

        let db = Arc::new(Mutex::new(db));
        // Optional and non-fatal: without the embedding model the other three detection
        // layers still work.
        detection.attach_semantic(&models, &db);

        Ok(Self {
            db,
            db_path,
            library_dir: data_dir.join("library"),
            models,
            audio,
            detection,
            capture: CaptureState::default(),
            session: SessionState::default(),
        })
    }

    /// Run `f` against the database.
    ///
    /// A poisoned mutex means an earlier command panicked while holding the lock. The
    /// guard is recovered rather than propagating the panic: SQLite statements are
    /// atomic, so the data itself is still consistent.
    pub fn with_db<T, E>(&self, f: impl FnOnce(&Db) -> Result<T, E>) -> Result<T, E> {
        let guard = self.db.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("database mutex was poisoned by an earlier panic; recovering");
            poisoned.into_inner()
        });
        f(&guard)
    }

    pub fn db_handle(&self) -> Arc<Mutex<Db>> {
        Arc::clone(&self.db)
    }

    pub fn audio(&self) -> &AudioState {
        &self.audio
    }

    pub fn audio_handle(&self) -> Arc<AudioState> {
        Arc::clone(&self.audio)
    }

    pub fn detection(&self) -> &DetectionState {
        &self.detection
    }

    pub fn detection_handle(&self) -> Arc<DetectionState> {
        Arc::clone(&self.detection)
    }

    pub fn capture(&self) -> &CaptureState {
        &self.capture
    }

    pub fn session(&self) -> &SessionState {
        &self.session
    }

    pub fn models(&self) -> &ModelStore {
        &self.models
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn library_dir(&self) -> &Path {
        &self.library_dir
    }
}
