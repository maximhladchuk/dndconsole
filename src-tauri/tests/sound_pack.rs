//! Installs the bundled sound pack for real.
//!
//! No API key: the preview CDN takes no authentication, which is the whole reason the
//! manifest is resolved and committed ahead of time. This test therefore runs for
//! anybody with a network connection, and is the proof that a user without a Freesound
//! account gets working sounds.
//!
//! Skipped when `DNDSOUND_OFFLINE` is set, for building on a machine with no network.

use std::path::PathBuf;
use std::sync::Mutex;

use dndsound_lib::sound_pack;
use dndsound_pack::{Manifest, THEMES};
use dndsound_store::Db;

fn offline() -> bool {
    if std::env::var_os("DNDSOUND_OFFLINE").is_some() {
        eprintln!("skipping: DNDSOUND_OFFLINE is set");
        return true;
    }
    false
}

fn cache_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dndsound-pack-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// `install` takes the mutex, not the database, so it can let go of the lock between
/// downloads. Tests hold the same shape and reach through it for their assertions.
fn seeded_db() -> Mutex<Db> {
    let db = Db::open_in_memory().expect("db");
    db.events().seed_if_empty().expect("seed");
    Mutex::new(db)
}

#[test]
fn installing_the_pack_gives_every_event_playable_sounds() {
    if offline() {
        return;
    }

    let db = seeded_db();
    let dir = cache_dir("full");

    let report = sound_pack::install(&db, &dir, |_| {}).expect("install");

    assert!(
        report.is_complete(),
        "every bundled sound must download: {:?}",
        report.failed
    );
    assert_eq!(report.downloaded, Manifest::bundled().sounds.len());
    assert_eq!(report.reused, 0, "nothing was cached before this run");
    assert_eq!(report.groups.len(), THEMES.len());

    for theme in THEMES {
        let event = db
            .lock()
            .unwrap()
            .events()
            .get(theme.event_id)
            .expect("event");
        let group_id = event
            .sound_group_id
            .unwrap_or_else(|| panic!("{} has no sound group", theme.event_id));

        let members = db
            .lock()
            .unwrap()
            .sounds()
            .group_members(group_id)
            .expect("members");
        assert_eq!(members.len(), theme.sound_ids.len(), "{}", theme.event_id);

        for sound in &members {
            // Probed from the file on disk, so this failing means the download is not
            // actually playable rather than merely present.
            assert!(
                sound.duration_ms.unwrap_or(0) > 100,
                "{} probed as {} ms",
                sound.display_name,
                sound.duration_ms.unwrap_or(0)
            );
            assert!(PathBuf::from(&sound.file_path).is_file());
            assert_eq!(sound.provenance.license, "CC0");
            assert!(
                sound.provenance.attribution.is_empty(),
                "CC0 obliges no credit, so none should be recorded"
            );
        }
    }

    // No partial files left behind.
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .expect("read cache")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".part"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "an interrupted download was left behind"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sound_from_an_older_pack_is_removed() {
    if offline() {
        return;
    }

    let db = seeded_db();
    let dir = cache_dir("prune");

    // A sound an earlier version of the pack installed, no longer in the manifest — and
    // deliberately under a licence this version does not allow.
    let group = db
        .lock()
        .unwrap()
        .sounds()
        .create_group("Old group")
        .expect("group");
    let stale = db
        .lock()
        .unwrap()
        .sounds()
        .import(&dndsound_store::sounds::NewSound {
            display_name: "Retired".into(),
            file_path: "/tmp/dndsound-retired.mp3".into(),
            managed: true,
            format: "mp3".into(),
            duration_ms: Some(1000),
            sample_rate: Some(44_100),
            channels: Some(2),
            provenance: dndsound_store::sounds::Provenance {
                source: "freesound".into(),
                source_id: "999999999".into(),
                source_url: "https://freesound.org/s/999999999/".into(),
                license: "CC-BY".into(),
                author: "somebody".into(),
                attribution: "\"Retired\" by somebody".into(),
            },
        })
        .expect("import");
    db.lock()
        .unwrap()
        .sounds()
        .add_to_group(group.id, stale.id)
        .expect("add");

    // A file the user brought themselves must survive untouched.
    let mine = db
        .lock()
        .unwrap()
        .sounds()
        .import(&dndsound_store::sounds::NewSound {
            display_name: "Mine".into(),
            file_path: "/tmp/dndsound-mine.wav".into(),
            managed: false,
            format: "wav".into(),
            duration_ms: Some(500),
            sample_rate: Some(44_100),
            channels: Some(1),
            provenance: dndsound_store::sounds::Provenance::local(),
        })
        .expect("import");

    let report = sound_pack::install(&db, &dir, |_| {}).expect("install");

    assert_eq!(
        report.pruned, 1,
        "the retired sound should have been removed"
    );
    assert!(
        db.lock().unwrap().sounds().get(stale.id).is_err(),
        "it is still there"
    );
    assert!(
        db.lock().unwrap().sounds().get(mine.id).is_ok(),
        "a local file was deleted"
    );
    assert!(
        db.lock()
            .unwrap()
            .sounds()
            .group_by_name("Old group")
            .expect("query")
            .is_none(),
        "the emptied group should have gone too"
    );

    for sound in db.lock().unwrap().sounds().list().expect("list") {
        assert!(
            sound.provenance.source != "freesound" || sound.provenance.license == "CC0",
            "{} is {} — the pack promises CC0 only",
            sound.display_name,
            sound.provenance.license
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn installing_twice_reuses_the_cache_and_duplicates_nothing() {
    if offline() {
        return;
    }

    let db = seeded_db();
    let dir = cache_dir("twice");

    let first = sound_pack::install(&db, &dir, |_| {}).expect("first");
    let before = db.lock().unwrap().sounds().count().expect("count");

    let second = sound_pack::install(&db, &dir, |_| {}).expect("second");

    assert_eq!(second.downloaded, 0, "the second run hit the network");
    assert_eq!(second.reused, first.downloaded);
    assert_eq!(
        db.lock().unwrap().sounds().count().expect("count"),
        before,
        "sounds were duplicated"
    );

    for theme in THEMES {
        let group = db
            .lock()
            .unwrap()
            .sounds()
            .group_by_name(theme.group_name)
            .expect("group")
            .expect("exists");
        assert_eq!(
            db.lock()
                .unwrap()
                .sounds()
                .group_members(group.id)
                .expect("members")
                .len(),
            theme.sound_ids.len(),
            "{} gained duplicate members",
            theme.group_name
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_truncated_cached_file_is_refetched_rather_than_imported() {
    if offline() {
        return;
    }

    let db = seeded_db();
    let dir = cache_dir("truncated");
    sound_pack::install(&db, &dir, |_| {}).expect("first");

    // Simulate a crash mid-write: a file that exists and is nonsense.
    let manifest = Manifest::bundled();
    let victim = &manifest.sounds[0];
    let path = sound_pack::cache_path(&dir, victim);
    std::fs::write(&path, b"not an mp3").expect("corrupt");

    let report = sound_pack::install(&db, &dir, |_| {}).expect("second");

    assert!(report.is_complete(), "recovery failed: {:?}", report.failed);
    assert_eq!(
        report.downloaded, 1,
        "the damaged file should have been refetched"
    );
    assert!(
        std::fs::metadata(&path).expect("metadata").len() > 1_000,
        "the file was not replaced"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn progress_is_reported_for_every_sound_in_order() {
    if offline() {
        return;
    }

    let db = seeded_db();
    let dir = cache_dir("progress");

    let mut seen = Vec::new();
    sound_pack::install(&db, &dir, |p| seen.push((p.done, p.total))).expect("install");

    let total = Manifest::bundled().sounds.len();
    assert_eq!(seen.len(), total);
    assert_eq!(seen.first(), Some(&(1, total)));
    assert_eq!(seen.last(), Some(&(total, total)));

    let _ = std::fs::remove_dir_all(&dir);
}
