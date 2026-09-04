//! Tauri wiring.
//!
//! This crate stays thin on purpose: it owns application state, exposes commands, and
//! emits events. Every real decision lives in the workspace crates under `crates/`.

// `audio` and `library` are public so the integration tests in `tests/` can drive the
// real import and playback paths without going through Tauri's command layer.
pub mod audio;
pub mod capture;
pub mod detection;
pub mod freesound;
pub mod library;
pub mod semantic;
pub mod session;
pub mod sound_pack;

mod commands;
mod error;
mod state;

pub use error::CommandError;

use tauri::Manager;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::state::AppState;

pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Startup failures (an unwritable data directory, a failed migration) must
            // reach the user as an explicit error rather than a window that half works.
            let state = AppState::initialize(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_status,
            commands::app::get_settings,
            commands::app::save_settings,
            commands::app::list_profiles,
            commands::app::create_profile,
            commands::app::set_active_profile,
            commands::app::delete_profile,
            commands::library::list_sounds,
            commands::library::import_sounds,
            commands::library::import_sound_directory,
            commands::library::rename_sound,
            commands::library::set_sound_volume,
            commands::library::set_sound_weight,
            commands::library::set_sound_enabled,
            commands::library::set_sound_favorite,
            commands::library::delete_sound,
            commands::library::sound_tags,
            commands::library::set_sound_tags,
            commands::library::rescan_sounds,
            commands::library::list_sound_groups,
            commands::library::create_sound_group,
            commands::library::update_sound_group,
            commands::library::delete_sound_group,
            commands::library::sound_group_members,
            commands::library::add_sound_to_group,
            commands::library::remove_sound_from_group,
            commands::playback::preview_sound,
            commands::playback::play_sound_group,
            commands::playback::start_ambience,
            commands::playback::stop_ambience,
            commands::playback::stop_all_sounds,
            commands::playback::playback_status,
            commands::microphone::list_microphones,
            commands::microphone::start_listening,
            commands::microphone::stop_listening,
            commands::microphone::capture_status,
            commands::session::start_session,
            commands::session::stop_session,
            commands::session::session_status,
            commands::session::simulate_transcript,
            commands::session::reset_detection_history,
            commands::session::run_recorded_audio,
            commands::events::list_events,
            commands::events::get_event,
            commands::events::save_event,
            commands::events::set_event_enabled,
            commands::events::set_event_sound_group,
            commands::events::delete_event,
            commands::events::reset_event,
            commands::events::restore_seed_events,
            commands::freesound::freesound_status,
            commands::freesound::set_freesound_key,
            commands::freesound::freesound_search,
            commands::freesound::freesound_import,
            commands::freesound::attribution_lines,
            commands::sound_pack::sound_pack_status,
            commands::sound_pack::install_sound_pack,
            commands::models::list_models,
            commands::models::download_model,
            commands::models::verify_model,
            commands::models::delete_model,
            commands::models::model_directory,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}

fn init_logging() {
    // Overridable with RUST_LOG, e.g. RUST_LOG=dndsound=debug,dndsound_store=trace
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,dndsound=debug,dndsound_store=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true))
        .init();
}
