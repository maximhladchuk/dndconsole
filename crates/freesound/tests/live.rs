//! Tests against the real Freesound API.
//!
//! Skipped, loudly, when `FREESOUND_API_KEY` is not set — there is no offline stand-in
//! for "does the service still return what we parse", and a mocked HTTP server would
//! only prove that the mock matches the parser.

use std::env;

use dndsound_freesound::{Client, License, SearchQuery};

fn client() -> Option<Client> {
    match env::var("FREESOUND_API_KEY") {
        Ok(key) if !key.trim().is_empty() => Some(Client::new(key)),
        _ => {
            eprintln!("skipping: FREESOUND_API_KEY is not set");
            None
        }
    }
}

#[test]
fn a_search_returns_parsable_results_with_downloadable_previews() {
    let Some(client) = client() else { return };

    let page = client
        .search(&SearchQuery {
            text: "wooden door creak".to_string(),
            ..SearchQuery::default()
        })
        .expect("search");

    assert!(page.total > 0, "expected matches for a common effect");
    assert!(!page.results.is_empty());

    for sound in &page.results {
        assert!(sound.id > 0);
        assert!(!sound.name.is_empty());
        assert!(
            sound.preview_url.starts_with("https://"),
            "preview must be an absolute https URL: {}",
            sound.preview_url
        );
        assert!(
            sound.duration_s > 0.0,
            "{} reported no duration",
            sound.name
        );
        assert!(
            (0.3..=15.0).contains(&sound.duration_s),
            "the duration filter was ignored: {} is {} s",
            sound.name,
            sound.duration_s
        );
    }
}

#[test]
fn the_default_search_only_returns_licences_that_permit_commercial_use() {
    let Some(client) = client() else { return };

    let page = client
        .search(&SearchQuery {
            text: "thunder".to_string(),
            ..SearchQuery::default()
        })
        .expect("search");

    for sound in &page.results {
        assert!(
            sound.license.allows_commercial_use(),
            "{} came back as {:?}, which the default filter should have excluded",
            sound.name,
            sound.license
        );
    }
}

#[test]
fn a_preview_actually_downloads_and_is_an_mp3() {
    let Some(client) = client() else { return };

    let page = client
        .search(&SearchQuery {
            text: "sword swing".to_string(),
            licenses: vec![License::Cc0],
            page_size: 1,
            ..SearchQuery::default()
        })
        .expect("search");

    let Some(sound) = page.results.first() else {
        panic!("no CC0 sword sounds found; the query needs revisiting");
    };

    let directory = std::env::temp_dir().join("dndsound-freesound-test");
    let destination = directory.join(format!("{}.mp3", sound.id));
    let _ = std::fs::remove_file(&destination);

    let mut last_progress = 0;
    let path = client
        .download_preview(sound, &destination, |p| last_progress = p.downloaded_bytes)
        .expect("download");

    let bytes = std::fs::read(&path).expect("read back");
    assert!(
        bytes.len() > 1_000,
        "suspiciously small: {} bytes",
        bytes.len()
    );
    assert_eq!(
        last_progress as usize,
        bytes.len(),
        "progress must reach the full size"
    );

    // MP3 frames start with either an ID3 tag or a frame sync.
    let is_mp3 = bytes.starts_with(b"ID3") || (bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0);
    assert!(is_mp3, "not an MP3: first bytes {:02X?}", &bytes[..4]);

    // No partial files left behind.
    let leftovers: Vec<_> = std::fs::read_dir(&directory)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".part-"))
        .collect();
    assert!(leftovers.is_empty(), "left a partial file behind");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_bad_key_is_reported_as_unauthorized_rather_than_as_a_generic_failure() {
    let client = Client::new("definitely-not-a-valid-key");
    let error = client
        .search(&SearchQuery {
            text: "door".to_string(),
            ..SearchQuery::default()
        })
        .expect_err("a bad key must fail");

    assert!(
        matches!(error, dndsound_freesound::Error::Unauthorized),
        "got {error:?}"
    );
}
