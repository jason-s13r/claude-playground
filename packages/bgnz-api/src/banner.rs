//! The two Briscoe Group fascias, and where they answer.
//!
//! Briscoes and Rebel Sport are **one Magento deployment**, not two. The same
//! schema, the same host pool and the same operation names serve both; which
//! catalogue answers is decided by a single `store` request header, and the
//! storefront's `vary` confirms it -- `Accept-Encoding,Store,Content-Currency,
//! Authorization,X-Magento-Cache-Id`. Either public host will answer for either
//! fascia, so the origin below is a matter of looking like the right client
//! rather than of reaching the right backend.
//!
//! What genuinely differs is the catalogue, the search index and the identity
//! site. The SKU namespaces are disjoint -- Briscoes numbers products
//! `1xxxxxx` and Rebel Sport `8xxxxxx`, and asking one fascia for the other's
//! SKU answers `total_count: 0` -- so a banner is a switch, never a fan-out.
//! There is nothing here to compare side by side the way the two Foodstuffs
//! banners sell the same groceries.
//!
//! The Klevu and Gigya keys are deliberately *not* here. Both are published by
//! the storefront's own `storeConfig`, so [`crate::Client`] reads them at run
//! time rather than carrying a copy that goes stale silently. See
//! [`crate::domain::Storefront`].

use std::fmt;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Banner {
    Briscoes,
    RebelSport,
}

impl Default for Banner {
    /// Briscoes, because a bare `bgnz` has to mean something and this tool is
    /// named after the group whose homeware fascia most people arrive for.
    fn default() -> Banner {
        Banner::Briscoes
    }
}

impl Banner {
    pub const ALL: [Banner; 2] = [Banner::Briscoes, Banner::RebelSport];

    /// Stable machine-readable name: config keys, state directories, JSON.
    ///
    /// The same spelling as [`Banner::store_code`] today, and still a separate
    /// method: one is this tool's vocabulary and the other is Magento's, and
    /// the day they diverge should cost one line rather than a hunt.
    pub fn id(self) -> &'static str {
        match self {
            Banner::Briscoes => "briscoes",
            Banner::RebelSport => "rebelsport",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Banner::Briscoes => "Briscoes",
            Banner::RebelSport => "Rebel Sport",
        }
    }

    /// The value of the `store` header.
    ///
    /// The one header that must never be omitted. Magento does not reject a
    /// request without it -- it serves the default store, which is Briscoes --
    /// so a forgotten header on a Rebel Sport command answers with homeware
    /// rather than an error.
    pub fn store_code(self) -> &'static str {
        match self {
            Banner::Briscoes => "briscoes",
            Banner::RebelSport => "rebelsport",
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
            "b" | "bri" | "bris" | "briscoes" | "briscoe" => Some(Banner::Briscoes),
            "r" | "reb" | "rebel" | "rebelsport" | "rs" => Some(Banner::RebelSport),
            _ => None,
        }
    }
}

impl fmt::Display for Banner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The hosts a banner is reached on.
///
/// Plain fields, not resolved from the environment: this crate takes values.
/// The caller decides whether an override exists and where it came from, which
/// is also how a test points the whole flow at a mock server.
///
/// `gigya` is the identity host and is shared by both fascias -- one SAP
/// Customer Data Cloud datacenter, `au1`, hosting two *sites*. The site is
/// chosen by API key, not by host, which is why there is no Gigya entry in
/// [`Banner`] and why signing in to one fascia does not sign in to the other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    /// The storefront: GraphQL, the availability API, media.
    pub origin: String,
    /// SAP Customer Data Cloud, where a password is exchanged for a session.
    pub gigya: String,
}

impl Endpoints {
    pub fn defaults(banner: Banner) -> Endpoints {
        Endpoints {
            origin: match banner {
                Banner::Briscoes => "https://www.briscoes.co.nz".into(),
                Banner::RebelSport => "https://www.rebelsport.co.nz".into(),
            },
            gigya: "https://accounts.au1.gigya.com".into(),
        }
    }

    pub fn with_origin(mut self, origin: impl Into<String>) -> Endpoints {
        self.origin = trim(origin.into());
        self
    }

    pub fn with_gigya(mut self, gigya: impl Into<String>) -> Endpoints {
        self.gigya = trim(gigya.into());
        self
    }

    /// The one GraphQL endpoint. Everything the catalogue and the account do
    /// goes through here.
    pub fn graphql(&self) -> String {
        format!("{}/graphql", self.origin)
    }

    /// Click-and-collect availability, which is a plain REST endpoint rather
    /// than a GraphQL field -- a separate service behind the same host.
    pub fn availability(&self) -> String {
        format!("{}/api/v1/web/cnc-commerce/availability", self.origin)
    }

    /// A Gigya REST method, `accounts.login` and the like.
    pub fn gigya(&self, method: &str) -> String {
        format!("{}/{method}", self.gigya)
    }

    /// Klevu's search endpoint, on the host `storeConfig` named.
    ///
    /// Takes the host because it is published per fascia and can move: the
    /// shard in `aucs34.ksearchnet.com` is an assignment, not a constant.
    pub fn klevu_search(&self, host: &str) -> String {
        let host = host.trim_end_matches('/');
        if host.starts_with("http://") || host.starts_with("https://") {
            format!("{host}/cs/v2/search")
        } else {
            format!("https://{host}/cs/v2/search")
        }
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
        for s in ["b", "bris", "briscoes", "Briscoes", "BRISCOE"] {
            assert_eq!(Banner::parse(s), Some(Banner::Briscoes), "{s}");
        }
        for s in [
            "r",
            "rebel",
            "rebelsport",
            "Rebel Sport",
            "rebel-sport",
            "rs",
        ] {
            assert_eq!(Banner::parse(s), Some(Banner::RebelSport), "{s}");
        }
        assert_eq!(Banner::parse("kmart"), None);
    }

    #[test]
    fn the_store_code_is_what_the_header_must_carry() {
        // Getting this wrong does not fail, it silently answers with the other
        // fascia's catalogue, so it is worth pinning.
        assert_eq!(Banner::Briscoes.store_code(), "briscoes");
        assert_eq!(Banner::RebelSport.store_code(), "rebelsport");
    }

    #[test]
    fn a_bare_command_means_briscoes() {
        assert_eq!(Banner::default(), Banner::Briscoes);
    }

    #[test]
    fn the_two_fascias_are_different_origins_but_one_identity_host() {
        let b = Endpoints::defaults(Banner::Briscoes);
        let r = Endpoints::defaults(Banner::RebelSport);
        assert_ne!(b.origin, r.origin);
        assert_eq!(b.gigya, r.gigya, "one Gigya datacenter serves both");
    }

    #[test]
    fn overrides_trim_a_trailing_slash() {
        let e = Endpoints::defaults(Banner::Briscoes).with_origin("http://127.0.0.1:8080/");
        assert_eq!(e.graphql(), "http://127.0.0.1:8080/graphql");
        assert_eq!(
            e.gigya("accounts.login"),
            "https://accounts.au1.gigya.com/accounts.login",
            "untouched"
        );
    }

    #[test]
    fn a_klevu_host_may_arrive_bare_or_with_a_scheme() {
        // `storeConfig` publishes it bare (`aucs34.ksearchnet.com`); a test
        // pointing at a mock server has to supply a full URL.
        let e = Endpoints::defaults(Banner::Briscoes);
        assert_eq!(
            e.klevu_search("aucs34.ksearchnet.com"),
            "https://aucs34.ksearchnet.com/cs/v2/search"
        );
        assert_eq!(
            e.klevu_search("http://127.0.0.1:9/"),
            "http://127.0.0.1:9/cs/v2/search"
        );
    }
}
