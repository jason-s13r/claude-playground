//! How this client presents itself.
//!
//! Unlike The Warehouse, neither of these storefronts is behind a bot manager.
//! Measured against both live sites on 2026-09-11: the GraphQL endpoint, the
//! availability API and Klevu all answer a cold client with no cookies, no
//! session bootstrap and no particular user agent. The obfuscated
//! `izysjitu.briscoes.co.nz` subdomain in a browser capture is a first-party
//! proxy for analytics, not a defence, and the only `__cf_bm` cookies in that
//! capture came from Square and Afterpay.
//!
//! So the emulation profile here is **not load-bearing** the way
//! `twlnz_api::EMULATION` is -- nothing has been observed to depend on it. It
//! is set anyway, because presenting as a real browser costs nothing and the
//! day one of these sites does turn a bot manager on, the client that already
//! looks like Firefox is the one that keeps working.

use net_kit::wreq_util::Profile;
use net_kit::ClientSpec;

/// The browser this client presents as, at every layer.
pub const EMULATION: Profile = Profile::Firefox139;

/// A profile by name, for the `BGNZ_EMULATION` escape hatch.
///
/// Matched against the enum's own variant names rather than a table written
/// here, so upgrading `wreq-util` brings its new browsers with it and this does
/// not have to be edited. Punctuation and case are ignored: `Firefox139`,
/// `firefox139` and `firefox-139` all name the same profile.
pub fn profile(name: &str) -> Option<Profile> {
    let wanted = squash(name);
    Profile::VARIANTS
        .iter()
        .copied()
        .find(|p| squash(&format!("{p:?}")) == wanted)
}

fn squash(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The client, **not following redirects**.
///
/// Every endpoint this crate calls answers directly. A redirect would mean the
/// site had started doing something new -- an interstitial, a challenge, a
/// moved host -- and surfacing that as an unexpected status is more useful than
/// quietly following it and decoding whatever came back.
pub fn client_spec() -> ClientSpec {
    client_spec_for(EMULATION)
}

/// The same client, presenting as some other browser.
pub fn client_spec_for(profile: Profile) -> ClientSpec {
    ClientSpec::new(profile, net_kit::wreq::redirect::Policy::none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_profile_names_a_real_browser() {
        let ua = net_kit::http::user_agent(EMULATION);
        assert!(ua.contains("Mozilla"), "{ua}");
        assert!(ua.contains("Firefox"), "{ua}");
    }

    #[test]
    fn a_profile_is_found_however_its_name_is_punctuated() {
        assert_eq!(profile("firefox139"), Some(Profile::Firefox139));
        assert_eq!(profile("Firefox_139"), Some(Profile::Firefox139));
        assert_eq!(profile("FIREFOX-139"), Some(Profile::Firefox139));
        assert_eq!(profile("netscape"), None);
    }
}
