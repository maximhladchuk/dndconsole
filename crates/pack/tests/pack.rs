//! The committed manifest has to agree with the catalog, with the seed events, and with
//! the licence promise made in the README.
//!
//! None of this needs the network: the manifest is data in the repository, and that is
//! the point — a user installs the pack without a Freesound account.

use std::collections::HashSet;

use dndsound_pack::{Manifest, THEMES};

#[test]
fn every_bundled_sound_is_public_domain() {
    // The application promises CC0 only, so nothing here may oblige attribution.
    for sound in &Manifest::bundled().sounds {
        assert!(
            sound.is_cc0(),
            "{} ({}) is licensed {}, not CC0",
            sound.name,
            sound.id,
            sound.license_url
        );
    }
}

#[test]
fn the_manifest_resolves_exactly_the_catalog() {
    let manifest = Manifest::bundled();

    let catalogued: HashSet<u64> = THEMES
        .iter()
        .flat_map(|t| t.sound_ids.iter().copied())
        .collect();
    let resolved: HashSet<u64> = manifest.sounds.iter().map(|s| s.id).collect();

    let missing: Vec<_> = catalogued.difference(&resolved).collect();
    let extra: Vec<_> = resolved.difference(&catalogued).collect();

    assert!(
        missing.is_empty(),
        "the manifest needs regenerating; missing {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "the manifest has sounds the catalog dropped: {extra:?}"
    );
}

#[test]
fn every_theme_points_at_a_real_seed_event() {
    let events: HashSet<String> = dndsound_detect::seed_events()
        .into_iter()
        .map(|e| e.id)
        .collect();

    for theme in THEMES {
        assert!(
            events.contains(theme.event_id),
            "{} has no matching event in seed_events()",
            theme.event_id
        );
    }
}

#[test]
fn every_seed_event_has_sounds_to_play() {
    let manifest = Manifest::bundled();

    for event in dndsound_detect::seed_events() {
        let count = manifest.sounds_for(&event.id).count();
        assert!(
            count >= 4,
            "{} has {count} sounds; a group needs a few or it repeats immediately",
            event.id
        );
    }
}

#[test]
fn no_sound_is_shared_between_themes() {
    let mut seen = HashSet::new();
    for theme in THEMES {
        for id in theme.sound_ids {
            assert!(seen.insert(id), "freesound {id} appears in two themes");
        }
    }
}

#[test]
fn every_entry_has_something_downloadable() {
    for sound in &Manifest::bundled().sounds {
        assert!(
            sound.preview_url.starts_with("https://cdn.freesound.org/"),
            "{} has an unexpected preview URL: {}",
            sound.id,
            sound.preview_url
        );
        assert!(
            sound.duration_s > 0.1,
            "{} reports {} s",
            sound.id,
            sound.duration_s
        );
        assert!(
            !sound.author.is_empty(),
            "{} has no author recorded",
            sound.id
        );
    }
}

#[test]
fn the_whole_pack_is_a_reasonable_first_launch_download() {
    let manifest = Manifest::bundled();
    let megabytes = manifest.total_bytes_estimate() as f64 / 1_048_576.0;

    assert!(
        megabytes < 25.0,
        "the pack is {megabytes:.1} MB; first launch would be a wait, not a download"
    );
    assert!(
        manifest.sounds.len() >= 60,
        "only {} sounds",
        manifest.sounds.len()
    );
}

#[test]
fn group_names_are_distinct() {
    let names: HashSet<&str> = THEMES.iter().map(|t| t.group_name).collect();
    assert_eq!(names.len(), THEMES.len(), "two themes share a group name");
}
