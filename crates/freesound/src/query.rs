//! The request and response types for a text search.

use serde::{Deserialize, Serialize};

use crate::license::License;

/// The licences worth offering by default.
///
/// `CC-BY-NC` is deliberately absent: it forbids commercial use, and a Dungeon Master
/// streaming a paid game would be doing exactly that without knowing it. It can still be
/// searched for explicitly.
pub const DEFAULT_LICENSES: &[License] = &[License::Cc0, License::CcBy, License::CcBySa];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub text: String,
    /// Shortest and longest acceptable sound, in seconds.
    pub min_duration: f32,
    pub max_duration: f32,
    /// Empty means "any licence".
    pub licenses: Vec<License>,
    pub page: u32,
    pub page_size: u32,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            // A one-shot effect. Long ambience is a separate search, not a default.
            min_duration: 0.3,
            max_duration: 15.0,
            licenses: DEFAULT_LICENSES.to_vec(),
            page: 1,
            page_size: 20,
        }
    }
}

impl SearchQuery {
    /// The `filter` parameter Freesound expects: a Solr-style expression.
    pub(crate) fn filter(&self) -> String {
        let mut parts = vec![format!(
            "duration:[{} TO {}]",
            self.min_duration, self.max_duration
        )];

        if !self.licenses.is_empty() {
            let names: Vec<String> = self
                .licenses
                .iter()
                .filter_map(|licence| freesound_license_name(licence).map(|n| format!("\"{n}\"")))
                .collect();
            if !names.is_empty() {
                parts.push(format!("license:({})", names.join(" OR ")));
            }
        }

        parts.join(" ")
    }
}

/// Freesound filters on its own human-readable licence names, not on the URLs it returns.
fn freesound_license_name(licence: &License) -> Option<&'static str> {
    match licence {
        License::Cc0 => Some("Creative Commons 0"),
        License::CcBy => Some("Attribution"),
        License::CcBySa => Some("Attribution Share Alike"),
        License::CcByNc => Some("Attribution Noncommercial"),
        License::SamplingPlus => Some("Sampling+"),
        License::Other(_) => None,
    }
}

/// One search result, reduced to what the library actually stores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sound {
    pub id: u64,
    pub name: String,
    pub username: String,
    pub license: License,
    pub duration_s: f32,
    /// The high-quality MP3 preview. This is what gets downloaded.
    pub preview_url: String,
    /// The sound's page, for attribution and for "show me this on Freesound".
    pub page_url: String,
}

impl Sound {
    /// The credit line a CC-BY licence requires, in the form Freesound asks for.
    pub fn attribution(&self) -> String {
        format!(
            "\"{}\" by {} — {} ({})",
            self.name,
            self.username,
            self.page_url,
            self.license.short_name()
        )
    }

    /// Parse one entry of the `results` array.
    pub(crate) fn from_json(value: &serde_json::Value) -> Option<Self> {
        let id = value.get("id")?.as_u64()?;
        Some(Sound {
            id,
            name: value.get("name")?.as_str()?.to_string(),
            username: value
                .get("username")?
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            license: License::from_url(value.get("license")?.as_str().unwrap_or("")),
            duration_s: value
                .get("duration")
                .and_then(|d| d.as_f64())
                .unwrap_or(0.0) as f32,
            preview_url: value
                .get("previews")?
                .get("preview-hq-mp3")?
                .as_str()?
                .to_string(),
            page_url: format!("https://freesound.org/s/{id}/"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub total: u64,
    pub page: u32,
    pub results: Vec<Sound>,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_filter_restricts_duration_and_licence() {
        let filter = SearchQuery::default().filter();
        assert!(filter.contains("duration:[0.3 TO 15]"), "got {filter}");
        assert!(filter.contains("Creative Commons 0"), "got {filter}");
        assert!(
            !filter.contains("Noncommercial"),
            "non-commercial licences must not be offered by default: {filter}"
        );
    }

    #[test]
    fn an_empty_licence_list_means_no_licence_filter() {
        let query = SearchQuery {
            licenses: Vec::new(),
            ..SearchQuery::default()
        };
        assert!(!query.filter().contains("license:"));
    }

    #[test]
    fn a_result_is_parsed_from_the_shape_the_api_actually_returns() {
        // Copied verbatim from a live response.
        let json: serde_json::Value = serde_json::from_str(
            r#"{
                "id": 778448,
                "name": "Wooden_Door_Creaking_03",
                "license": "http://creativecommons.org/publicdomain/zero/1.0/",
                "type": "wav",
                "filesize": 159746,
                "duration": 1.10862,
                "username": "BlondPanda",
                "previews": {
                    "preview-hq-mp3": "https://cdn.freesound.org/previews/778/778448_8927049-hq.mp3",
                    "preview-lq-mp3": "https://cdn.freesound.org/previews/778/778448_8927049-lq.mp3"
                }
            }"#,
        )
        .expect("fixture");

        let sound = Sound::from_json(&json).expect("parse");
        assert_eq!(sound.id, 778448);
        assert_eq!(sound.username, "BlondPanda");
        assert_eq!(sound.license, License::Cc0);
        assert_eq!(sound.page_url, "https://freesound.org/s/778448/");
        assert!(sound.preview_url.ends_with("-hq.mp3"));
        assert!((sound.duration_s - 1.108).abs() < 0.01);
    }

    #[test]
    fn a_result_without_a_high_quality_preview_is_skipped_rather_than_faked() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"id": 1, "name": "x", "username": "y", "license": "", "previews": {}}"#,
        )
        .expect("fixture");
        assert!(Sound::from_json(&json).is_none());
    }

    #[test]
    fn attribution_names_the_author_the_sound_and_the_licence() {
        let sound = Sound {
            id: 778448,
            name: "Wooden_Door_Creaking_03".to_string(),
            username: "BlondPanda".to_string(),
            license: License::CcBy,
            duration_s: 1.1,
            preview_url: String::new(),
            page_url: "https://freesound.org/s/778448/".to_string(),
        };

        let credit = sound.attribution();
        assert!(credit.contains("BlondPanda"));
        assert!(credit.contains("Wooden_Door_Creaking_03"));
        assert!(credit.contains("freesound.org/s/778448"));
        assert!(credit.contains("CC-BY"));
    }
}
