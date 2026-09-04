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
/// **This one is load-bearing and it is not interchangeable.** Measured against
/// the live site: `Firefox151` and `Safari26_4` are served the page, while
/// `Firefox149` and `Chrome149` are both answered with a 403 managed challenge
/// on the home page itself. So this is not a preference -- an older profile
/// does not degrade, it stops working entirely, and the failure is a bot check
/// rather than anything the rest of this crate can interpret.
pub const EMULATION: Profile = Profile::Firefox151;

/// Firefox, **following redirects**.
///
/// The opposite of the Woolworths client, and deliberately so. Here a redirect
/// is ordinary traffic rather than a bot check: a keyword search 302s into a
/// category page whenever the term matches one, and the form login answers 302
/// with the session cookies attached. Refusing to follow would break both.
pub fn client_spec() -> ClientSpec {
    ClientSpec::new(EMULATION, net_kit::wreq::redirect::Policy::limited(10))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_agent_is_a_firefox() {
        let ua = net_kit::http::user_agent(EMULATION);
        assert!(ua.contains("Firefox"), "{ua}");
    }
}
