//! Which licence a Freesound sound carries, and what that obliges the user to do.
//!
//! This is not bookkeeping for its own sake. Most of Freesound is Creative Commons, and
//! the difference between CC0 and CC-BY is the difference between "use it however you
//! like" and "you must name the author wherever this is heard". A sound library that
//! forgets which is which cannot honour either.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum License {
    /// Public domain dedication. No obligations at all.
    Cc0,
    /// Attribution required.
    CcBy,
    /// Attribution required, derivatives must carry the same licence.
    CcBySa,
    /// Attribution required, no commercial use.
    CcByNc,
    /// Freesound's older "Sampling+" licence: attribution, no commercial use as-is.
    SamplingPlus,
    /// Anything not recognised. Carries the URL so the user can read it themselves.
    Other(String),
}

impl License {
    /// Freesound reports the licence as a Creative Commons URL.
    pub fn from_url(url: &str) -> Self {
        // Both http and https appear in the API's output, and the version segment
        // changes over time (3.0 and 4.0 are both common). Matching on the licence path
        // rather than the whole URL survives both.
        let path = url.trim_end_matches('/').to_lowercase();

        if path.contains("publicdomain/zero") {
            License::Cc0
        } else if path.contains("sampling+") {
            License::SamplingPlus
        } else if path.contains("/licenses/by-nc-sa") || path.contains("/licenses/by-nc") {
            License::CcByNc
        } else if path.contains("/licenses/by-sa") {
            License::CcBySa
        } else if path.contains("/licenses/by") {
            License::CcBy
        } else {
            License::Other(url.to_string())
        }
    }

    pub fn requires_attribution(&self) -> bool {
        !matches!(self, License::Cc0)
    }

    /// False only where the licence positively forbids it. `Other` is unknown rather
    /// than permitted, so it answers false too: the safe direction for a claim like this
    /// is to understate.
    pub fn allows_commercial_use(&self) -> bool {
        matches!(self, License::Cc0 | License::CcBy | License::CcBySa)
    }

    pub fn short_name(&self) -> &str {
        match self {
            License::Cc0 => "CC0",
            License::CcBy => "CC-BY",
            License::CcBySa => "CC-BY-SA",
            License::CcByNc => "CC-BY-NC",
            License::SamplingPlus => "Sampling+",
            License::Other(_) => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_licence_urls_freesound_actually_returns_are_recognised() {
        // Taken verbatim from live API responses.
        assert_eq!(
            License::from_url("http://creativecommons.org/publicdomain/zero/1.0/"),
            License::Cc0
        );
        assert_eq!(
            License::from_url("http://creativecommons.org/licenses/by/4.0/"),
            License::CcBy
        );
        assert_eq!(
            License::from_url("https://creativecommons.org/licenses/by/3.0/"),
            License::CcBy
        );
        assert_eq!(
            License::from_url("http://creativecommons.org/licenses/by-nc/4.0/"),
            License::CcByNc
        );
        assert_eq!(
            License::from_url("http://creativecommons.org/licenses/by-sa/3.0/"),
            License::CcBySa
        );
        assert_eq!(
            License::from_url("http://creativecommons.org/licenses/sampling+/1.0/"),
            License::SamplingPlus
        );
    }

    #[test]
    fn by_nc_sa_is_treated_as_non_commercial_not_as_share_alike() {
        // Order matters in the matcher: by-nc-sa contains both "by-nc" and "by-sa", and
        // the restriction that matters more is the commercial one.
        assert_eq!(
            License::from_url("http://creativecommons.org/licenses/by-nc-sa/4.0/"),
            License::CcByNc
        );
    }

    #[test]
    fn an_unknown_licence_keeps_its_url_and_claims_nothing() {
        let licence = License::from_url("https://example.com/some-bespoke-terms");
        assert!(matches!(licence, License::Other(_)));
        assert!(licence.requires_attribution());
        assert!(!licence.allows_commercial_use());
    }

    #[test]
    fn only_cc_zero_is_free_of_obligations() {
        assert!(!License::Cc0.requires_attribution());
        for licence in [
            License::CcBy,
            License::CcBySa,
            License::CcByNc,
            License::SamplingPlus,
        ] {
            assert!(licence.requires_attribution(), "{licence:?}");
        }
    }
}
