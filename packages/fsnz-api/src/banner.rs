//! The two Foodstuffs banners, and where they answer.

use std::fmt;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Banner {
    NewWorld,
    PaknSave,
}

impl Banner {
    pub const ALL: [Banner; 2] = [Banner::NewWorld, Banner::PaknSave];

    /// Stable machine-readable name: config keys, state directories, JSON.
    pub fn id(self) -> &'static str {
        match self {
            Banner::NewWorld => "newworld",
            Banner::PaknSave => "paknsave",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Banner::NewWorld => "New World",
            Banner::PaknSave => "PAK'nSAVE",
        }
    }

    /// The code Foodstuffs' own login uses for this banner.
    ///
    /// This is what a minted token must be scoped to. A token carrying `NAT`
    /// instead is not rejected by the cart -- it authenticates and answers with
    /// an empty cart belonging to nobody, which is far worse than a refusal.
    pub fn code(self) -> &'static str {
        match self {
            Banner::NewWorld => "MNW",
            Banner::PaknSave => "PNS",
        }
    }

    /// Accepts the spellings people actually type.
    pub fn parse(s: &str) -> Option<Banner> {
        let key: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match key.as_str() {
            "nw" | "newworld" => Some(Banner::NewWorld),
            "pns" | "pak" | "pakn" | "paknsave" | "packnsave" => Some(Banner::PaknSave),
            _ => None,
        }
    }
}

impl fmt::Display for Banner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The hostnames a banner is reached on.
///
/// Plain fields, not resolved from the environment: this crate takes values.
/// The caller decides whether an override exists and where it came from, which
/// is also how a test points the whole flow at a mock server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    /// The storefront, which mints the guest token.
    pub origin: String,
    /// The JSON API the storefront's own frontend calls.
    pub api: String,
}

impl Endpoints {
    pub fn defaults(banner: Banner) -> Endpoints {
        match banner {
            Banner::NewWorld => Endpoints {
                origin: "https://www.newworld.co.nz".into(),
                api: "https://api-prod.newworld.co.nz".into(),
            },
            Banner::PaknSave => Endpoints {
                origin: "https://www.paknsave.co.nz".into(),
                api: "https://api-prod.paknsave.co.nz".into(),
            },
        }
    }

    /// Replace either host, trailing slash trimmed so joins stay clean.
    pub fn with_origin(mut self, origin: impl Into<String>) -> Endpoints {
        self.origin = trim(origin.into());
        self
    }

    pub fn with_api(mut self, api: impl Into<String>) -> Endpoints {
        self.api = trim(api.into());
        self
    }
}

/// Club Plus, which is one login across both banners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClubPlusEndpoints {
    pub login: String,
    pub api: String,
}

impl Default for ClubPlusEndpoints {
    fn default() -> ClubPlusEndpoints {
        ClubPlusEndpoints {
            login: "https://login.clubplus.co.nz".into(),
            api: "https://api-prod.clubplus.co.nz/retail-fsl-online-edge".into(),
        }
    }
}

impl ClubPlusEndpoints {
    pub fn with_login(mut self, login: impl Into<String>) -> ClubPlusEndpoints {
        self.login = trim(login.into());
        self
    }

    pub fn with_api(mut self, api: impl Into<String>) -> ClubPlusEndpoints {
        self.api = trim(api.into());
        self
    }
}

fn trim(s: String) -> String {
    s.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_spellings_people_type() {
        for s in ["nw", "NW", "newworld", "New World", "new-world"] {
            assert_eq!(Banner::parse(s), Some(Banner::NewWorld), "{s}");
        }
        for s in ["pns", "paknsave", "PAK'nSAVE", "pak n save", "Pack n Save"] {
            assert_eq!(Banner::parse(s), Some(Banner::PaknSave), "{s}");
        }
        assert_eq!(Banner::parse("countdown"), None);
    }

    #[test]
    fn the_banner_codes_are_what_a_token_must_be_scoped_to() {
        assert_eq!(Banner::NewWorld.code(), "MNW");
        assert_eq!(Banner::PaknSave.code(), "PNS");
    }

    #[test]
    fn overrides_trim_a_trailing_slash() {
        let e = Endpoints::defaults(Banner::NewWorld).with_api("http://127.0.0.1:8080/");
        assert_eq!(e.api, "http://127.0.0.1:8080");
        assert_eq!(e.origin, "https://www.newworld.co.nz", "untouched");
    }

    #[test]
    fn the_two_banners_are_different_hosts() {
        assert_ne!(
            Endpoints::defaults(Banner::NewWorld),
            Endpoints::defaults(Banner::PaknSave)
        );
    }
}
