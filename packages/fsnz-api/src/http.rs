//! How this client presents itself.

use std::sync::Arc;

use net_kit::wreq_util::Profile;
use net_kit::{ClientSpec, Jar};

/// The browser emulated. Handshake, HTTP/2 settings and headers all derive from
/// it, and the OS rides along -- macOS on every host, so one build is one
/// device. Club Plus scores device identity, so changing this costs one
/// verification.
///
/// Not the same profile the Woolworths client uses, and not interchangeable:
/// the two storefronts sit behind different bot managers.
pub const EMULATION: Profile = Profile::Chrome137;

/// Cloudflare's cookies, and the storefront's credentials. Nothing else --
/// analytics, the store picker and UI state have no business in a credential
/// store.
pub fn cookie_keep(name: &str) -> bool {
    name.starts_with("__cf")
        || name.starts_with("_cf")
        || name == "cf_clearance"
        || matches!(name, "fs-user-token" | "refresh_token" | "API_TOKEN")
}

/// Chrome, a persistent jar, and redirects followed.
///
/// The jar matters: Cloudflare's `__cf_bm` marks a session it has already
/// scored, and a cold start is scored as a new visitor. Redirects are followed
/// because the storefront uses them normally -- the Woolworths client
/// deliberately does the opposite, and the two must not be swapped.
pub fn client_spec(jar: Arc<Jar>) -> ClientSpec {
    ClientSpec::new(EMULATION, net_kit::wreq::redirect::Policy::limited(10)).with_cookies(jar)
}

/// The `User-Agent` the emulation sends.
///
/// Needed because the SSO exchange echoes it back in the *request body* as
/// `fingerprintGuest`, which is the one place it has to be named.
pub fn user_agent() -> String {
    net_kit::http::user_agent(EMULATION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_cloudflares_cookies_and_the_storefronts_credentials_only() {
        for name in [
            "__cf_bm",
            "_cfuvid",
            "cf_clearance",
            "fs-user-token",
            "refresh_token",
        ] {
            assert!(cookie_keep(name), "{name} should be kept");
        }
        // Analytics and UI state, which do not belong in a credential store.
        for name in [
            "_dyid",
            "_dy_soct",
            "STORE_ID_V2",
            "Region",
            "orderDetailsTCs",
        ] {
            assert!(!cookie_keep(name), "{name} should not be kept");
        }
    }

    #[test]
    fn the_user_agent_is_a_real_browsers() {
        let ua = user_agent();
        assert!(ua.contains("Chrome"), "{ua}");
    }
}
