//! The identifiers Kmart's own front end sends, and the only place they live.
//!
//! None of these is a secret. Every one is served to every visitor in the HTML
//! of the storefront's home page, which is how they were obtained and how they
//! are re-obtained. They are gathered here anyway, for two reasons.
//!
//! The first is that they rot. A vendor rotates a key, registers a new Auth0
//! application, or bumps a client version, and the failure that follows is
//! remote from the cause -- an empty catalogue, or a login that is refused
//! without explanation. Keeping them in one file means the answer to "did they
//! change something?" is one diff rather than a search.
//!
//! The second is that they are high-entropy strings in a public repository,
//! which is what a leaked credential also looks like. A scanner cannot tell
//! the difference and should not be asked to: this file is named in
//! `.github/secret_scanning.yml` so the alerts land nowhere, and the price of
//! that exemption is that **nothing that is actually secret may be added
//! here**. Credentials belong in the keyring, via `net_kit::Secrets`.
//!
//! To refresh the lot -- both hosts answer a plain client, no browser needed:
//!
//! ```text
//! curl -s https://www.kmart.co.nz/ | grep -o '"constructorApiKey":"[^"]*"'
//! curl -s https://www.kmart.co.nz/ | grep -o '"auth0Configs":{[^}]*}'
//! curl -s https://www.kmart.com.au/ | grep -o '"constructorApiKey":"[^"]*"'
//! curl -s https://www.kmart.com.au/ | grep -o '"auth0Configs":{[^}]*}'
//! ```

use crate::country::Country;

// ------------------------------------------------------------------ Auth0

/// The Auth0 tenant. One for both countries -- an account made on either
/// storefront signs in to the other.
pub const AUTH_DOMAIN: &str = "https://auth.kmart.com.au";

/// What an access token is minted *for*, and the reason one sign-in serves
/// both countries: the audience names the gateway family rather than a
/// country, and is identical on both storefronts. An `api.kmart.co.nz`
/// audience is refused.
pub const AUTH_AUDIENCE: &str = "https://api.kmart.com.au/gateway/graphql";

/// `offline_access` is the one that matters: it is what makes a session
/// renewable without a password.
pub const AUTH_SCOPE: &str = "openid profile email offline_access";

/// The Auth0 application a storefront signs in as.
///
/// **One per country, despite the shared tenant.** Measured, and not
/// guessable from the pair: the two ids have nothing in common. A refresh
/// token is bound to the application that minted it, so renewing an
/// Australian session with the New Zealand id is refused -- which is why
/// [`crate::StoredSession`] records which storefront a session came from.
///
/// Public by construction: a single-page app cannot keep a secret, so Auth0
/// identifies it rather than authenticating it, and PKCE is what makes the
/// exchange safe without one.
pub fn auth_client_id(country: Country) -> &'static str {
    match country {
        Country::Au => "wf0Dz7lKuhKN319si7vRE0DiPyEyE5Hy",
        Country::Nz => "dFXRMOiN5flCI5VLc8IZ005AsGklQro4",
    }
}

/// Where Auth0 sends the authorization code. Never fetched -- the code is read
/// off the redirect -- but the code is bound to it, so it has to match what
/// that country's storefront registered.
pub fn auth_redirect(country: Country) -> String {
    format!("{}/account/login", country.origin())
}

// ---------------------------------------------------------- Constructor.io

/// The Constructor.io index key.
///
/// These are the whole difference between the two catalogues. Using the
/// Australian key against a New Zealand postcode is not an error anything
/// reports -- it just quietly answers with Australian products at Australian
/// prices.
///
/// Read-only, and issued per index for the browser to send. The key that can
/// *write* to an index is a different one, and is not here.
pub fn search_key(country: Country) -> &'static str {
    match country {
        Country::Au => "key_GZTqlLr41FS2p7AY",
        Country::Nz => "key_EyiTrcbGw7IFH3wR",
    }
}

/// The client version Constructor is told it is talking to.
///
/// Sent as `c` on every request. Constructor uses it for its own analytics
/// rather than for gating, but a value the index has never seen is worth
/// avoiding, so this is the one the storefront sends.
pub const SEARCH_CLIENT: &str = "ciojs-client-2.77.1";

// ------------------------------------------------------- the resolved set

/// What a country's front end is configured with, as this crate needs it.
///
/// A value rather than a lookup, so the caller decides where it came from:
/// [`Vendor::baked`] is what shipped, [`Vendor::read`] is what the storefront
/// is serving today. The CLI prefers the live one and caches it, and falls
/// back to the baked one -- a rotation should heal itself, and Kmart having a
/// bad afternoon should not stop a search working.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Vendor {
    pub search_key: String,
    pub auth_client_id: String,
    pub auth_audience: String,
    pub auth_redirect: String,
}

impl Vendor {
    /// The values compiled in, as measured when this was written.
    pub fn baked(country: Country) -> Vendor {
        Vendor {
            search_key: search_key(country).into(),
            auth_client_id: auth_client_id(country).into(),
            auth_audience: AUTH_AUDIENCE.into(),
            auth_redirect: auth_redirect(country),
        }
    }

    /// Read them out of a storefront home page.
    ///
    /// Deliberately a substring search rather than a parse: the page carries
    /// a megabyte of Next.js payload whose shape is Kmart's business, and the
    /// three keys wanted from it have been stable across every capture. All
    /// or nothing -- a page that yields two of the three is a page whose shape
    /// has moved, and half a configuration is worse than the baked one.
    pub fn read(html: &str, country: Country) -> Option<Vendor> {
        let field = |name: &str| -> Option<String> {
            let at = html.find(&format!("\"{name}\":\""))? + name.len() + 4;
            let rest = &html[at..];
            let end = rest.find('"')?;
            match rest[..end].trim() {
                "" => None,
                value => Some(value.to_string()),
            }
        };
        Some(Vendor {
            search_key: field("constructorApiKey")?,
            auth_client_id: field("clientId")?,
            auth_audience: field("audience")?,
            auth_redirect: auth_redirect(country),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_here_is_shared_between_the_countries_by_accident() {
        // Both of these were shared until the Australian storefront was read
        // rather than assumed to match. The failure was silent in one case
        // (the other country's prices) and remote from its cause in the other
        // (a refused renewal, hours after a login that worked).
        assert_ne!(auth_client_id(Country::Au), auth_client_id(Country::Nz));
        assert_ne!(search_key(Country::Au), search_key(Country::Nz));
        assert_ne!(auth_redirect(Country::Au), auth_redirect(Country::Nz));
    }

    #[test]
    fn the_tenant_and_the_audience_are_shared_on_purpose() {
        // This is what lets one sign-in serve both countries.
        assert!(AUTH_DOMAIN.ends_with("kmart.com.au"));
        assert!(AUTH_AUDIENCE.starts_with("https://api.kmart.com.au"));
        assert!(AUTH_SCOPE.contains("offline_access"), "renewable");
    }

    #[test]
    fn a_redirect_belongs_to_its_own_storefront() {
        assert_eq!(
            auth_redirect(Country::Nz),
            "https://www.kmart.co.nz/account/login"
        );
    }

    /// A trimmed home page, in the shape both storefronts serve.
    const PAGE: &str = r#"<!doctype html><html><body><script>window.__NUXT__={
        "runtimeConfig":{"public":{"auth0Configs":{"domain":"auth.kmart.com.au",
        "clientId":"AaBbCcDdEeFf0011","audience":"https://api.kmart.com.au/gateway/graphql"}},
        "preview":false,"constructorApiKey":"key_TestTestTest"}}</script></body></html>"#;

    #[test]
    fn a_live_page_supplies_the_whole_set() {
        let read = Vendor::read(PAGE, Country::Nz).expect("all three keys are there");
        assert_eq!(read.search_key, "key_TestTestTest");
        assert_eq!(read.auth_client_id, "AaBbCcDdEeFf0011");
        assert_eq!(
            read.auth_audience,
            "https://api.kmart.com.au/gateway/graphql"
        );
        // Not read from the page: the storefront knows its own address, and
        // the redirect is ours to construct from the country.
        assert_eq!(read.auth_redirect, auth_redirect(Country::Nz));
    }

    #[test]
    fn a_page_missing_any_of_them_supplies_none_of_them() {
        // Half a configuration is worse than the baked one: it would pair a
        // fresh key with a stale client id and fail somewhere else entirely.
        assert!(Vendor::read(
            &PAGE.replace("constructorApiKey", "somethingElse"),
            Country::Nz
        )
        .is_none());
        assert!(Vendor::read(&PAGE.replace("clientId", "somethingElse"), Country::Nz).is_none());
        assert!(Vendor::read("<html>not the page you wanted</html>", Country::Nz).is_none());
    }

    #[test]
    fn an_empty_value_is_not_a_value() {
        assert!(Vendor::read(&PAGE.replace("key_TestTestTest", ""), Country::Nz).is_none());
    }

    #[test]
    fn the_baked_set_is_what_the_accessors_say() {
        let baked = Vendor::baked(Country::Au);
        assert_eq!(baked.search_key, search_key(Country::Au));
        assert_eq!(baked.auth_client_id, auth_client_id(Country::Au));
    }
}
