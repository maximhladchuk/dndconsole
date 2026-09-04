//! The resolved form of the catalog: every sound with the URL it downloads from.
//!
//! Freesound's preview CDN needs no authentication, but its URLs contain a per-sound
//! hash that cannot be derived from the id. Resolving ids to URLs takes an API key.
//! Doing that once, here, and committing the result means the *user* never needs a
//! Freesound account: first launch downloads straight from the CDN.
//!
//! Regenerate with `cargo run -p dndsound-pack --example build_manifest` (needs
//! `FREESOUND_API_KEY`). The generator refuses anything that is not CC0.

use serde::{Deserialize, Serialize};

const MANIFEST_JSON: &str = include_str!("../manifest.json");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestSound {
    pub id: u64,
    pub event_id: String,
    pub name: String,
    pub author: String,
    pub duration_s: f32,
    pub preview_url: String,
    /// Kept so a test can prove every entry is still public domain.
    pub license_url: String,
}

impl ManifestSound {
    pub fn page_url(&self) -> String {
        format!("https://freesound.org/s/{}/", self.id)
    }

    pub fn is_cc0(&self) -> bool {
        self.license_url.contains("publicdomain/zero")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// When the ids were resolved, ISO-8601 date.
    pub generated: String,
    pub sounds: Vec<ManifestSound>,
}

impl Manifest {
    /// The committed manifest. Parsing cannot fail for a file the tests have checked.
    pub fn bundled() -> Self {
        serde_json::from_str(MANIFEST_JSON).expect("the bundled manifest is valid JSON")
    }

    pub fn sounds_for<'a>(
        &'a self,
        event_id: &'a str,
    ) -> impl Iterator<Item = &'a ManifestSound> + 'a {
        self.sounds.iter().filter(move |s| s.event_id == event_id)
    }

    pub fn total_bytes_estimate(&self) -> u64 {
        // 128 kbps MP3: 16 kB per second.
        self.sounds
            .iter()
            .map(|s| (s.duration_s * 16_000.0) as u64)
            .sum()
    }
}
