//! How this client presents itself.

use net_kit::wreq_util::Profile;
use net_kit::ClientSpec;

/// The browser this client presents as, at every layer.
///
/// Akamai Bot Manager sits in front of both Kmart hosts and scores the TLS
/// handshake, the HTTP/2 settings and the headers together, so they have to
/// agree. `wreq` derives all three from this one value -- including the
/// `User-Agent` -- which is why nothing here sets a user agent by hand: a
/// header naming a different Firefox than the handshake is exactly the
/// inconsistency being watched for.
pub const EMULATION: Profile = Profile::Firefox139;

/// Firefox, **no cookie jar**, and **no redirect policy**.
///
/// Nothing needs keeping between requests: the gateway is authorised by a
/// bearer header rather than a cookie, and the catalogue is anonymous. An
/// unexpected redirect here is a bot check, which must surface rather than be
/// quietly followed -- so redirects are opted into per request in exactly one
/// place, the login flow, which builds its own client with a jar.
pub fn client_spec() -> ClientSpec {
    ClientSpec::new(EMULATION, net_kit::wreq::redirect::Policy::none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_agent_is_a_firefox() {
        let ua = net_kit::http::user_agent(EMULATION);
        assert!(ua.contains("Firefox"), "{ua}");
        assert!(!ua.contains("Chrome"), "{ua}");
    }
}
