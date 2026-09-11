//! How this client presents itself.

use net_kit::wreq_util::Profile;
use net_kit::ClientSpec;

/// The browser this client presents as, at every layer.
///
/// Cloudflare sits in front of this storefront and scores the TLS handshake,
/// the HTTP/2 settings and the headers together, so they have to agree. `wreq`
/// derives all three from this one value -- including the `User-Agent` -- which
/// is why nothing here sets a user agent by hand.
///
/// **This one is load-bearing, and which value works changes.** Measured
/// against the live home page on 2026-09-11: `Safari18_5` and every `Safari26`
/// are served, and `Firefox142`, `Firefox148`, `Firefox151`, `Chrome141`,
/// `Chrome149` and `Edge145` are all answered with a 403 managed challenge.
/// That is a whole-family result rather than an old-version one -- `Firefox151`
/// is the newest the crate has and it is refused alongside `Firefox142` -- so
/// bumping the version is not the fix when this next flips. A refused profile
/// does not degrade, it stops working entirely, on the first request, and the
/// failure is a bot check nothing else in this crate can interpret.
///
/// `Firefox151` was the value here until the measurement above; it worked in
/// 2026-09 and does not now. [`profile`] is why that costs a variable rather
/// than a release.
pub const EMULATION: Profile = Profile::Safari26_4;

/// A profile by name, for the `TWLNZ_EMULATION` escape hatch.
///
/// Matched against the enum's own variant names rather than a table written
/// here, so upgrading `wreq-util` brings its new browsers with it and this does
/// not have to be edited. Punctuation and case are ignored: `Safari26_4`,
/// `safari26_4` and `safari-26-4` all name the same profile.
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

/// The client, **following redirects**.
///
/// The opposite of the Woolworths client, and deliberately so. Here a redirect
/// is ordinary traffic rather than a bot check: a keyword search 302s into a
/// category page whenever the term matches one, the account page 302s to the
/// sign-in page for a guest, and the form login answers 302 with the session
/// cookies attached. Refusing to follow would break all three.
pub fn client_spec() -> ClientSpec {
    client_spec_for(EMULATION)
}

/// The same client, presenting as some other browser.
pub fn client_spec_for(profile: Profile) -> ClientSpec {
    ClientSpec::new(profile, net_kit::wreq::redirect::Policy::limited(10))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_agent_is_a_safari() {
        let ua = net_kit::http::user_agent(EMULATION);
        assert!(ua.contains("Safari"), "{ua}");
        // Not a Chrome pretending: every Chrome UA carries "Safari" too, and
        // the Chrome profiles are the ones the live site refuses.
        assert!(!ua.contains("Chrome"), "{ua}");
    }

    #[test]
    fn a_profile_is_found_however_its_name_is_punctuated() {
        assert_eq!(profile("Safari26_4"), Some(Profile::Safari26_4));
        assert_eq!(profile("safari26_4"), Some(Profile::Safari26_4));
        assert_eq!(profile(" safari-26-4 "), Some(Profile::Safari26_4));
        assert_eq!(profile("firefox151"), Some(Profile::Firefox151));
    }

    #[test]
    fn an_unknown_name_is_none_rather_than_a_silent_default() {
        // The whole point of the override is to pick a profile that works, so
        // a typo quietly falling back to the one that does not would be the
        // worst possible answer.
        assert_eq!(profile("safari"), None);
        assert_eq!(profile("safari26_99"), None);
        assert_eq!(profile(""), None);
    }
}
