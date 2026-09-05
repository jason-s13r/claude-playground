//! Where Kmart lives -- which is four places.
//!
//! Plain fields, not resolved from the environment: this crate takes values.
//! The caller decides whether an override exists, which is also how a test
//! points the whole flow at one mock server.
//!
//! The four hosts are not interchangeable and only one of them is Kmart's.
//! [`Endpoints::search`] is Constructor.io, a third party running Kmart's
//! product index; it is open to anyone. The other three sit behind Akamai Bot
//! Manager. That asymmetry is the single most important fact about this crate
//! and it is why the catalogue works cold and the gateway does not.

pub use crate::vendor::{Vendor, AUTH_AUDIENCE, AUTH_SCOPE, SEARCH_CLIENT};

use crate::country::Country;
use crate::vendor;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    /// The storefront. Only its home page is reliably served to a
    /// non-browser; see [`crate::http`].
    pub origin: String,
    /// The GraphQL gateway.
    pub api: String,
    /// Auth0. Australian-domained for both countries: one tenant issues for
    /// the pair, which is why this does not move with the country.
    pub auth: String,
    /// Constructor.io. The one host here that is not Kmart's, and the one that
    /// answers a cold client. Shared by both countries -- they differ by index
    /// key, not by host.
    pub search: String,
    /// Which index that key selects.
    pub search_key: String,
    /// The Auth0 application to sign in as. Per country despite the shared
    /// tenant, and a refresh token is bound to it -- see [`vendor`].
    pub auth_client_id: String,
    /// What the authorization code is bound to. Never fetched.
    pub auth_redirect: String,
}

impl Endpoints {
    /// The hosts one country is served from.
    pub fn of(country: Country) -> Endpoints {
        Endpoints {
            origin: country.origin().into(),
            api: country.api().into(),
            auth: "https://auth.kmart.com.au".into(),
            search: "https://ac.cnstrc.com".into(),
            search_key: vendor::search_key(country).into(),
            auth_client_id: vendor::auth_client_id(country).into(),
            auth_redirect: vendor::auth_redirect(country),
        }
    }

    /// Take the values a storefront is serving today over the ones that
    /// shipped. What the CLI applies when its cache is warm.
    pub fn with_vendor(mut self, vendor: &Vendor) -> Endpoints {
        self.search_key = vendor.search_key.clone();
        self.auth_client_id = vendor.auth_client_id.clone();
        self.auth_redirect = vendor.auth_redirect.clone();
        self
    }

    pub fn with_origin(mut self, origin: impl Into<String>) -> Endpoints {
        self.origin = trim(origin.into());
        self
    }

    pub fn with_api(mut self, api: impl Into<String>) -> Endpoints {
        self.api = trim(api.into());
        self
    }

    pub fn with_auth(mut self, auth: impl Into<String>) -> Endpoints {
        self.auth = trim(auth.into());
        self
    }

    pub fn with_search(mut self, search: impl Into<String>) -> Endpoints {
        self.search = trim(search.into());
        self
    }

    pub fn with_search_key(mut self, key: impl Into<String>) -> Endpoints {
        self.search_key = key.into();
        self
    }

    /// Point every host at one place. For a test with a single mock server.
    pub fn all(base: impl Into<String>) -> Endpoints {
        let base = trim(base.into());
        Endpoints {
            origin: base.clone(),
            api: base.clone(),
            auth: base.clone(),
            search: base,
            search_key: "key_test".into(),
            auth_client_id: "client-test".into(),
            auth_redirect: "https://storefront.test/account/login".into(),
        }
    }

    pub fn graphql(&self) -> String {
        format!("{}/gateway/graphql", self.api)
    }

    /// Keyword search. Also the product lookup: a keycode is a term the index
    /// matches exactly, so there is no separate item endpoint to call.
    pub fn search_term(&self, term: &str) -> String {
        format!("{}/search/{}", self.search, encode(term))
    }

    /// A category listing, by the 32-hex group id a category is known by.
    pub fn browse_group(&self, group_id: &str) -> String {
        format!("{}/browse/group_id/{}", self.search, encode(group_id))
    }

    pub fn authorize(&self) -> String {
        format!("{}/authorize", self.auth)
    }

    pub fn token(&self) -> String {
        format!("{}/oauth/token", self.auth)
    }
}

fn trim(s: String) -> String {
    s.trim_end_matches('/').to_string()
}

/// Percent-encode one URL component.
///
/// Hand-rolled rather than taken from `url` because the only things encoded
/// here are a search term and a group id, and reaching for a URL type to join
/// a path would also re-encode the query string this crate builds by hand.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `a=1&b=2` from pairs, in the order given.
///
/// Order is kept because Constructor's filter parameters repeat one key --
/// `filters[Colour]` can appear twice -- and a map would lose the second.
pub fn query_string(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_trim_a_trailing_slash_and_leave_the_others_alone() {
        let e = Endpoints::of(Country::Nz).with_api("http://127.0.0.1:9/");
        assert_eq!(e.graphql(), "http://127.0.0.1:9/gateway/graphql");
        assert_eq!(e.search, "https://ac.cnstrc.com", "untouched");
    }

    #[test]
    fn all_points_every_host_at_one_place() {
        let e = Endpoints::all("http://127.0.0.1:9");
        assert_eq!(e.graphql(), "http://127.0.0.1:9/gateway/graphql");
        assert_eq!(e.token(), "http://127.0.0.1:9/oauth/token");
        assert_eq!(e.search_term("mop"), "http://127.0.0.1:9/search/mop");
    }

    #[test]
    fn the_two_countries_get_different_hosts_and_the_same_auth() {
        let nz = Endpoints::of(Country::Nz);
        let au = Endpoints::of(Country::Au);
        assert_eq!(nz.graphql(), "https://api.kmart.co.nz/gateway/graphql");
        assert_eq!(au.graphql(), "https://api.kmart.com.au/gateway/graphql");
        // One Auth0 tenant, Australian-domained, for both.
        assert_eq!(nz.auth, au.auth);
        assert_eq!(nz.search, au.search, "one index host");
        assert_ne!(nz.search_key, au.search_key, "two indexes");
    }

    #[test]
    fn a_search_term_is_encoded_into_the_path() {
        let e = Endpoints::of(Country::Nz);
        assert_eq!(
            e.search_term("milk frother"),
            "https://ac.cnstrc.com/search/milk%20frother"
        );
        // A keycode is just a term, which is what makes one product lookup and
        // a keyword search the same request.
        assert_eq!(
            e.search_term("43165537"),
            "https://ac.cnstrc.com/search/43165537"
        );
    }

    #[test]
    fn a_query_string_keeps_repeated_keys_in_the_order_given() {
        let params = [
            ("filters[Colour]".to_string(), "Black".to_string()),
            ("filters[Colour]".to_string(), "White".to_string()),
        ]
        .to_vec();
        assert_eq!(
            query_string(&params),
            "filters%5BColour%5D=Black&filters%5BColour%5D=White"
        );
    }
}
