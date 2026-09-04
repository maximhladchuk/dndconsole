//! End-to-end test of the Phase 2 path: import files, build a group, play from it.
//!
//! This drives the same code the UI drives, minus Tauri's command wrapper — a real
//! database, real files on disk, and the real audio engine. It plays audio very
//! quietly and briefly.

use std::path::{Path, PathBuf};

use dndsound_lib::audio::AudioState;
use dndsound_lib::library::{import_directory, ImportOptions};
use dndsound_sound::Bus;
use dndsound_store::{AppSettings, Db};

fn dev_sounds() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/dev-sounds")
        .canonicalize()
        .expect("dev sounds exist")
}

fn quiet_settings() -> AppSettings {
    AppSettings {
        master_volume: 0.02,
        ..AppSettings::default()
    }
}

/// Import the dev sounds and put the three door creaks into one group.
fn setup() -> (Db, dndsound_store::sounds::SoundGroup) {
    let db = Db::open_in_memory().expect("db");
    let options = ImportOptions {
        managed: false,
        library_dir: std::env::temp_dir().join("dndsound-flow-library"),
    };

    let report = import_directory(&db, &dev_sounds(), &options).expect("import");
    assert_eq!(report.imported.len(), 10, "expected ten dev sounds");

    let group = db.sounds().create_group("Wooden Doors").expect("group");
    for sound in report
        .imported
        .iter()
        .filter(|s| s.file_path.contains("door_wood_creak"))
    {
        db.sounds()
            .add_to_group(group.id, sound.id)
            .expect("add to group");
    }

    assert_eq!(
        db.sounds().group_members(group.id).expect("members").len(),
        3,
        "three door creaks"
    );

    (db, group)
}

#[test]
fn importing_a_folder_then_playing_a_group_reaches_every_sound_without_repeats() {
    let (db, group) = setup();
    let audio = AudioState::initialize(&quiet_settings());

    if !audio.snapshot().available {
        eprintln!("skipping: no audio output available");
        return;
    }

    let members = db.sounds().group_members(group.id).expect("members");

    let mut played = Vec::new();
    let mut previous: Option<i64> = None;

    for _ in 0..30 {
        let sound = audio
            .play_group(&group, &members, Bus::Sfx)
            .expect("play should succeed")
            .expect("the group has playable sounds");

        assert_ne!(
            Some(sound.id),
            previous,
            "anti-repeat is on by default, so the same file must not play twice in a row"
        );
        previous = Some(sound.id);
        played.push(sound.id);
    }

    let distinct: std::collections::HashSet<i64> = played.iter().copied().collect();
    assert_eq!(distinct.len(), 3, "all three door creaks should be reached");
}

#[test]
fn a_group_whose_sounds_are_all_disabled_plays_nothing_and_does_not_error() {
    let (db, group) = setup();
    let audio = AudioState::initialize(&quiet_settings());

    if !audio.snapshot().available {
        eprintln!("skipping: no audio output available");
        return;
    }

    for member in db.sounds().group_members(group.id).expect("members") {
        db.sounds().set_enabled(member.id, false).expect("disable");
    }

    let members = db.sounds().group_members(group.id).expect("members");
    let played = audio
        .play_group(&group, &members, Bus::Sfx)
        .expect("an empty group is a configuration state, not an error");

    assert!(played.is_none());
}

#[test]
fn a_sound_whose_file_disappeared_is_reported_rather_than_played() {
    let db = Db::open_in_memory().expect("db");
    let audio = AudioState::initialize(&quiet_settings());

    if !audio.snapshot().available {
        eprintln!("skipping: no audio output available");
        return;
    }

    let sound = db
        .sounds()
        .import(&dndsound_store::sounds::NewSound {
            display_name: "Gone".into(),
            file_path: "/definitely/not/here.wav".into(),
            managed: false,
            format: "wav".into(),
            duration_ms: None,
            sample_rate: None,
            channels: None,
            provenance: Default::default(),
        })
        .expect("import row");

    let err = audio
        .preview(&sound)
        .expect_err("missing file must be an error");
    assert_eq!(err.kind, "soundFileMissing");
    assert!(err.message.contains("here.wav"), "got {}", err.message);
}

#[test]
fn ambience_and_one_shots_coexist_and_are_reported_in_the_snapshot() {
    let (db, group) = setup();
    let audio = AudioState::initialize(&quiet_settings());

    if !audio.snapshot().available {
        eprintln!("skipping: no audio output available");
        return;
    }

    let rain = db
        .sounds()
        .list()
        .expect("sounds")
        .into_iter()
        .find(|s| s.file_path.contains("rain_loop"))
        .expect("the dev rain loop");

    audio.start_ambience("scene:rain", &rain).expect("ambience");

    let members = db.sounds().group_members(group.id).expect("members");
    audio
        .play_group(&group, &members, Bus::Sfx)
        .expect("play")
        .expect("a sound");

    let snapshot = audio.snapshot();
    assert_eq!(snapshot.active_ambience, vec!["scene:rain".to_string()]);
    assert_eq!(snapshot.active_one_shots, 1);
    assert!(snapshot.cache_used_bytes > 0);

    audio.stop_all().expect("stop");
    let after = audio.snapshot();
    assert!(after.active_ambience.is_empty());
    assert_eq!(after.active_one_shots, 0);
}

#[test]
fn sequential_groups_play_in_order_and_survive_a_settings_change() {
    let (db, mut group) = setup();
    let audio = AudioState::initialize(&quiet_settings());

    if !audio.snapshot().available {
        eprintln!("skipping: no audio output available");
        return;
    }

    group = db
        .sounds()
        .update_group(group.id, &group.name, "sequential", false, 1.0)
        .expect("switch to sequential");

    let members = db.sounds().group_members(group.id).expect("members");
    let expected: Vec<i64> = members.iter().map(|m| m.id).collect();

    let played: Vec<i64> = (0..6)
        .map(|_| {
            audio
                .play_group(&group, &members, Bus::Sfx)
                .expect("play")
                .expect("a sound")
                .id
        })
        .collect();

    let mut want = expected.clone();
    want.extend(expected.iter().copied());
    assert_eq!(
        played, want,
        "sequential order should follow group membership"
    );
}
