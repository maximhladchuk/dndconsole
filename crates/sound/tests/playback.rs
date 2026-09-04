//! Integration tests that drive the real audio backend.
//!
//! These open the default output device and genuinely play audio — at a very low gain,
//! briefly. That is the point: it is the only way to know the mixer graph, decoding,
//! streaming and cache actually work rather than merely compile.
//!
//! They are skipped (not failed) when no output device is available, so the suite still
//! runs on a machine or CI box without audio hardware.

use std::path::{Path, PathBuf};
use std::time::Duration;

use dndsound_sound::{Bus, Error, SoundEngine, Volumes};

/// Quiet enough not to startle anyone running the tests at night.
const TEST_GAIN: f32 = 0.02;

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/dev-sounds")
        .canonicalize()
        .expect("dev-sounds directory should exist; run assets/dev-sounds/generate.py")
}

fn asset(name: &str) -> PathBuf {
    assets().join(name)
}

/// Build an engine, or return `None` when the machine has no usable output device.
fn engine() -> Option<SoundEngine> {
    match SoundEngine::new() {
        Ok(mut engine) => {
            engine.set_volumes(Volumes {
                master: TEST_GAIN,
                ..Volumes::default()
            });
            Some(engine)
        }
        Err(e) => {
            eprintln!("skipping: no audio output available ({e})");
            None
        }
    }
}

#[test]
fn a_one_shot_plays_and_is_cached() {
    let Some(mut engine) = engine() else { return };

    engine
        .play_one_shot(asset("door_wood_creak_01.wav"), 1.0, Bus::Sfx)
        .expect("play should succeed");

    assert_eq!(engine.active_one_shots(), 1);
    assert!(
        engine.cache_used_bytes() > 0,
        "decoded audio should be cached"
    );

    let after_first = engine.cache_used_bytes();
    engine
        .play_one_shot(asset("door_wood_creak_01.wav"), 1.0, Bus::Sfx)
        .expect("replay should succeed");

    assert_eq!(
        engine.cache_used_bytes(),
        after_first,
        "playing the same file again must reuse the cached decode"
    );
}

#[test]
fn one_shots_overlap_rather_than_cutting_each_other_off() {
    let Some(mut engine) = engine() else { return };

    for name in [
        "sword_swing_01.wav",
        "sword_swing_02.wav",
        "sword_swing_03.wav",
    ] {
        engine
            .play_one_shot(asset(name), 1.0, Bus::Sfx)
            .expect("play should succeed");
    }

    assert_eq!(
        engine.active_one_shots(),
        3,
        "all three should be audible at once"
    );
}

#[test]
fn ambience_keeps_playing_underneath_a_one_shot() {
    let Some(mut engine) = engine() else { return };

    engine
        .start_ambience(
            "rain",
            asset("rain_loop_01.wav"),
            1.0,
            Duration::from_millis(50),
        )
        .expect("ambience should start");
    assert!(engine.is_ambience_playing("rain"));

    engine
        .play_one_shot(asset("thunder_01.wav"), 1.0, Bus::Sfx)
        .expect("play should succeed");

    // The one-shot lands on a different bus, so the bed is untouched.
    assert!(
        engine.is_ambience_playing("rain"),
        "rain must survive the thunder"
    );
    assert_eq!(engine.active_one_shots(), 1);

    engine.stop_ambience("rain", Duration::from_millis(50));
    assert!(!engine.is_ambience_playing("rain"));
    assert!(engine.active_ambience().is_empty());
}

#[test]
fn starting_the_same_ambience_key_twice_does_not_stack_it() {
    let Some(mut engine) = engine() else { return };

    for _ in 0..3 {
        engine
            .start_ambience(
                "rain",
                asset("rain_loop_01.wav"),
                1.0,
                Duration::from_millis(20),
            )
            .expect("ambience should start");
    }

    assert_eq!(engine.active_ambience(), vec!["rain".to_string()]);
}

#[test]
fn stopping_an_unknown_ambience_key_is_harmless() {
    let Some(mut engine) = engine() else { return };
    engine.stop_ambience("never-started", Duration::from_millis(10));
    assert!(engine.active_ambience().is_empty());
}

#[test]
fn a_missing_file_reports_which_file_is_missing() {
    let Some(mut engine) = engine() else { return };

    let path = asset("this_file_does_not_exist.wav");
    let err = engine
        .play_one_shot(&path, 1.0, Bus::Sfx)
        .expect_err("a missing file must be an error");

    match err {
        Error::Missing(missing) => assert_eq!(missing, path),
        other => panic!("expected Missing, got {other:?}"),
    }
    assert!(err_message_mentions_the_path(&path));
}

fn err_message_mentions_the_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    Error::Missing(path.to_path_buf())
        .to_string()
        .contains(name)
}

#[test]
fn an_undecodable_file_is_reported_rather_than_silently_ignored() {
    let Some(mut engine) = engine() else { return };

    let path = std::env::temp_dir().join("dndsound-not-really-audio.wav");
    std::fs::write(&path, b"this is not a wav file").expect("write temp file");

    let err = engine
        .play_one_shot(&path, 1.0, Bus::Sfx)
        .expect_err("garbage must not decode");
    assert!(matches!(err, Error::Decode { .. }), "got {err:?}");

    std::fs::remove_file(&path).ok();
}

#[test]
fn muting_and_volume_changes_apply_without_stopping_playback() {
    let Some(mut engine) = engine() else { return };

    engine
        .play_one_shot(asset("thunder_02.wav"), 1.0, Bus::Sfx)
        .expect("play should succeed");

    engine.set_volumes(Volumes {
        master: TEST_GAIN,
        muted: true,
        ..Volumes::default()
    });
    assert!(engine.volumes().muted);
    assert_eq!(
        engine.active_one_shots(),
        1,
        "muting must not stop the sound"
    );

    engine.set_volumes(Volumes {
        master: TEST_GAIN,
        muted: false,
        ..Volumes::default()
    });
    assert!(!engine.volumes().muted);
}

#[test]
fn the_cache_stays_within_its_budget_across_many_files() {
    // 2 MB is larger than the biggest single asset (thunder, ~1.1 MB decoded) but far
    // smaller than all of them together, so eviction has to do real work. A budget
    // below the largest single file would instead exercise the documented exception:
    // one oversized entry is kept rather than re-decoded on every play.
    const BUDGET: usize = 2 * 1024 * 1024;
    let Ok(mut engine) = SoundEngine::with_cache_budget(BUDGET) else {
        eprintln!("skipping: no audio output available");
        return;
    };
    engine.set_volumes(Volumes {
        master: TEST_GAIN,
        ..Volumes::default()
    });

    for name in [
        "door_wood_creak_01.wav",
        "door_wood_creak_02.wav",
        "door_wood_creak_03.wav",
        "thunder_01.wav",
        "thunder_02.wav",
        "wolf_growl_01.wav",
    ] {
        engine
            .play_one_shot(asset(name), 1.0, Bus::Sfx)
            .expect("play should succeed");
    }

    assert!(
        engine.cache_used_bytes() <= BUDGET,
        "cache grew to {} bytes, over the {BUDGET} byte budget",
        engine.cache_used_bytes()
    );
    assert!(
        engine.cache_used_bytes() > 0,
        "something should still be cached"
    );
}

#[test]
fn a_single_file_larger_than_the_budget_is_still_cached() {
    // Documented exception: refusing to cache it would mean decoding it on every play.
    let Ok(mut engine) = SoundEngine::with_cache_budget(64 * 1024) else {
        eprintln!("skipping: no audio output available");
        return;
    };
    engine.set_volumes(Volumes {
        master: TEST_GAIN,
        ..Volumes::default()
    });

    engine
        .play_one_shot(asset("thunder_01.wav"), 1.0, Bus::Sfx)
        .expect("play should succeed");

    assert!(
        engine.cache_used_bytes() > 64 * 1024,
        "the oversized entry should be retained"
    );
}

#[test]
fn stop_all_clears_everything() {
    let Some(mut engine) = engine() else { return };

    engine
        .start_ambience(
            "rain",
            asset("rain_loop_01.wav"),
            1.0,
            Duration::from_millis(20),
        )
        .expect("ambience should start");
    engine
        .play_one_shot(asset("sword_swing_01.wav"), 1.0, Bus::Sfx)
        .expect("play should succeed");

    engine.stop_all(Duration::from_millis(20));

    assert_eq!(engine.active_one_shots(), 0);
    assert!(engine.active_ambience().is_empty());
}
