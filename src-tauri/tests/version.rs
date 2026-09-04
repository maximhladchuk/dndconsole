//! The version is written in three files that cannot see each other.
//!
//! `Cargo.toml` builds the binary, `package.json` builds the frontend, and
//! `tauri.conf.json` is what the installer and the About box show. Nothing in the build
//! compares them, so a hand edit to one is invisible until someone reports the wrong
//! version in a bug report. `scripts/version.sh` writes all three; this fails if they
//! ever drift apart anyway.

use std::path::Path;

fn repo_root() -> &'static Path {
    // CARGO_MANIFEST_DIR is src-tauri/.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
}

/// The `"version"` field of a JSON file, without pulling in a parser for one string.
fn json_version(relative: &str) -> String {
    let path = repo_root().join(relative);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {relative}: {e}"));

    text.lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("\"version\": \"")
                .and_then(|rest| rest.split('"').next())
                .map(str::to_string)
        })
        .unwrap_or_else(|| panic!("no version field in {relative}"))
}

#[test]
fn every_file_that_carries_the_version_agrees() {
    let crate_version = env!("CARGO_PKG_VERSION");

    for file in ["package.json", "src-tauri/tauri.conf.json"] {
        assert_eq!(
            json_version(file),
            crate_version,
            "{file} and Cargo.toml disagree. Use scripts/version.sh rather than editing \
             them by hand."
        );
    }
}
