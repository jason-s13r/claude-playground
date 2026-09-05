//! Australia or New Zealand.
//!
//! Kmart runs one backend for both. The same GraphQL schema answers on two
//! hosts, store ids share a namespace across the pair -- asking the Australian
//! gateway about location `8229` describes a shop in Auckland -- and the login
//! is one Auth0 tenant, Australian-domained, for both countries.
//!
//! What actually differs is the catalogue and the money: each country has its
//! own Constructor.io index under its own key, with its own products at its
//! own prices. So the country is a parameter rather than a build of its own,
//! and it is threaded through everything that talks to Kmart.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Country {
    Au,
    Nz,
}

impl Country {
    pub const ALL: [Country; 2] = [Country::Au, Country::Nz];

    /// The `x-country-code` header, and the `country` field on every gateway
    /// input that takes one.
    pub fn code(self) -> &'static str {
        match self {
            Country::Au => "AU",
            Country::Nz => "NZ",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Country::Au => "Australia",
            Country::Nz => "New Zealand",
        }
    }

    pub fn currency(self) -> &'static str {
        match self {
            Country::Au => "AUD",
            Country::Nz => "NZD",
        }
    }

    /// The storefront, which is also the `Origin` the gateway expects. A
    /// mismatched origin is refused by CORS before Kmart sees it.
    pub fn origin(self) -> &'static str {
        match self {
            Country::Au => "https://www.kmart.com.au",
            Country::Nz => "https://www.kmart.co.nz",
        }
    }

    pub fn api(self) -> &'static str {
        match self {
            Country::Au => "https://api.kmart.com.au",
            Country::Nz => "https://api.kmart.co.nz",
        }
    }

    /// The Constructor.io index key that shipped for this country.
    ///
    /// A convenience over [`crate::vendor::search_key`], which is where the
    /// value and the reasons live. Prefer [`crate::Endpoints::search_key`],
    /// which may carry a fresher one read off the storefront.
    pub fn search_key(self) -> &'static str {
        crate::vendor::search_key(self)
    }

    /// The domain a cookie export is filtered to.
    pub fn cookie_domain(self) -> &'static str {
        match self {
            Country::Au => "kmart.com.au",
            Country::Nz => "kmart.co.nz",
        }
    }

    /// What the site calls the subdivision a postcode is in.
    ///
    /// A real state in Australia (`VIC`), an island in New Zealand (`NI`).
    /// Worth naming because the listing filter differs with it: the New
    /// Zealand index carries `Available in NI` and `Available in SI` facets
    /// and the Australian one does not.
    pub fn subdivision(self) -> &'static str {
        match self {
            Country::Au => "state",
            Country::Nz => "island",
        }
    }

    pub fn parse(text: &str) -> Option<Country> {
        match text.trim().to_ascii_lowercase().as_str() {
            "au" | "aus" | "australia" => Some(Country::Au),
            "nz" | "nzl" | "new zealand" | "new-zealand" => Some(Country::Nz),
            _ => None,
        }
    }
}

impl fmt::Display for Country {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Country::Au => "au",
            Country::Nz => "nz",
        })
    }
}

impl FromStr for Country {
    type Err = String;

    fn from_str(s: &str) -> Result<Country, String> {
        Country::parse(s).ok_or_else(|| format!("{s:?} is not a country; use `au` or `nz`"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_countries_never_share_a_catalogue_key() {
        // The failure this guards against is silent: the wrong key answers
        //200 with the other country's products at the other country's prices.
        assert_ne!(Country::Au.search_key(), Country::Nz.search_key());
        assert_ne!(Country::Au.api(), Country::Nz.api());
        assert_ne!(Country::Au.origin(), Country::Nz.origin());
    }

    #[test]
    fn a_country_parses_from_what_someone_would_type() {
        assert_eq!(Country::parse("nz"), Some(Country::Nz));
        assert_eq!(Country::parse("AU"), Some(Country::Au));
        assert_eq!(Country::parse(" Australia "), Some(Country::Au));
        assert_eq!(Country::parse("New Zealand"), Some(Country::Nz));
        assert_eq!(Country::parse("uk"), None);
    }

    #[test]
    fn display_round_trips_through_parse() {
        for c in Country::ALL {
            assert_eq!(Country::parse(&c.to_string()), Some(c));
        }
    }

    #[test]
    fn it_serialises_as_the_short_code_a_config_file_would_hold() {
        assert_eq!(serde_json::to_string(&Country::Nz).unwrap(), r#""nz""#);
        assert_eq!(
            serde_json::from_str::<Country>(r#""au""#).unwrap(),
            Country::Au
        );
    }
}
