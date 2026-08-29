//! The two Foodstuffs banners this tool talks to.
//!
//! New World and PAK'nSAVE are both Foodstuffs NZ and run the same online
//! platform, so one client drives both -- they differ only in which hostnames
//! they answer on.

use anyhow::{bail, Result};
use std::env;
use std::fmt;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Banner {
    NewWorld,
    PaknSave,
}

/// The hostnames a banner is reached on. Both are overridable from the
/// environment: these are undocumented endpoints, so when Foodstuffs moves one
/// a user should be able to follow without waiting for a release. The tests
/// point them at a local mock server.
#[derive(Clone, Debug)]
pub struct Endpoints {
    /// The storefront, which mints the guest token.
    pub origin: String,
    /// The JSON API the storefront's own frontend calls.
    pub api: String,
}

impl Banner {
    pub const ALL: [Banner; 2] = [Banner::NewWorld, Banner::PaknSave];

    /// Stable machine-readable name: config keys, state directories, `--json`.
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

    /// The code Foodstuffs' own login uses for this banner (the `banner=`
    /// parameter on login.clubplus.co.nz).
    pub fn code(self) -> &'static str {
        match self {
            Banner::NewWorld => "MNW",
            Banner::PaknSave => "PNS",
        }
    }

    fn default_origin(self) -> &'static str {
        match self {
            Banner::NewWorld => "https://www.newworld.co.nz",
            Banner::PaknSave => "https://www.paknsave.co.nz",
        }
    }

    fn default_api(self) -> &'static str {
        match self {
            Banner::NewWorld => "https://api-prod.newworld.co.nz",
            Banner::PaknSave => "https://api-prod.paknsave.co.nz",
        }
    }

    fn env_key(self, suffix: &str) -> String {
        match self {
            Banner::NewWorld => format!("FSNZ_NEWWORLD_{suffix}"),
            Banner::PaknSave => format!("FSNZ_PAKNSAVE_{suffix}"),
        }
    }

    /// Tokens are scoped to one banner: the API rejects a New World token
    /// presented with a PAK'nSAVE store. Commands touching both banners
    /// therefore need one variable each.
    pub fn token_env_key(self) -> String {
        self.env_key("TOKEN")
    }

    pub fn endpoints(self) -> Endpoints {
        let pick = |suffix: &str, fallback: &str| {
            env::var(self.env_key(suffix))
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| fallback.to_string())
                .trim_end_matches('/')
                .to_string()
        };
        Endpoints {
            origin: pick("ORIGIN", self.default_origin()),
            api: pick("API", self.default_api()),
        }
    }

    /// Accepts the spellings people actually type.
    pub fn parse(s: &str) -> Result<Banner> {
        let key: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match key.as_str() {
            "nw" | "newworld" => Ok(Banner::NewWorld),
            "pns" | "pak" | "paknsave" | "packnsave" | "pakn" => Ok(Banner::PaknSave),
            _ => bail!("unknown banner '{s}' (expected 'newworld'/'nw' or 'paknsave'/'pns')"),
        }
    }
}

impl fmt::Display for Banner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for Banner {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Banner::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_spellings_people_type() {
        for s in ["nw", "NW", "newworld", "New World", "new-world"] {
            assert_eq!(Banner::parse(s).unwrap(), Banner::NewWorld, "{s}");
        }
        for s in ["pns", "paknsave", "PAK'nSAVE", "pak n save", "Pack n Save"] {
            assert_eq!(Banner::parse(s).unwrap(), Banner::PaknSave, "{s}");
        }
        assert!(Banner::parse("countdown").is_err());
    }

    #[test]
    fn ids_are_distinct_and_stable() {
        assert_eq!(Banner::NewWorld.id(), "newworld");
        assert_eq!(Banner::PaknSave.id(), "paknsave");
    }
}
