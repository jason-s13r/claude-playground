//! What Kmart said no with.
//!
//! Two cases are worth naming rather than passing through as a status code.
//!
//! GraphQL answers 200 with an `errors` array rather than an HTTP status, so
//! "not signed in" arrives as an extension code on a successful response --
//! [`Error::Graphql`] and [`Error::SessionExpired`] come out of a body, not a
//! header.
//!
//! And Kmart's own hosts are behind Akamai Bot Manager, which does not answer
//! `403 Forbidden` like a permission check. It answers a `429` carrying
//! `cpr_chlge`, or `200` with an interstitial page whose body is a sensor
//! script. Both look like success or like rate limiting to anything that only
//! reads the status, so [`Error::Challenged`] exists to say the real thing:
//! this needs cookies from a browser, and waiting will not help.

use net_kit::{AuthFault, Fault, HttpError};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] HttpError),

    #[error(transparent)]
    Net(#[from] net_kit::Error),

    /// Akamai Bot Manager served a challenge instead of the thing asked for.
    ///
    /// Not retryable and not a rate limit, though it wears a `429`: the
    /// challenge wants a sensor payload computed by the script it ships, and
    /// no number of retries produces one. Importing cookies from a signed-in
    /// browser is the way past it.
    #[error("{host} answered with a bot challenge rather than the request")]
    Challenged { host: String },

    #[error("the Kmart session has expired")]
    SessionExpired,

    #[error("not signed in to Kmart")]
    NotSignedIn,

    /// There are cookies but no way to turn them into a bearer token: the
    /// refresh token is gone or was refused, and the Auth0 session cookies
    /// that would mint a new one are absent or stale.
    #[error("the stored Kmart session cannot be renewed")]
    SessionUnrenewable,

    #[error("{operation} failed: {message}")]
    Graphql {
        operation: &'static str,
        message: String,
    },

    /// A cart mutation lost a race: `version` no longer matched. The caller
    /// re-reads the cart and applies the change to the version it got back.
    #[error("the cart changed underneath this request")]
    CartConflict,

    #[error("no store with id {0}")]
    NoSuchStore(String),

    #[error("no product with keycode {0}")]
    NoSuchProduct(String),

    #[error("no category matching {0:?}")]
    NoSuchCategory(String),

    #[error("Kmart is rate limiting this client")]
    RateLimited { retry_after: Option<u64> },

    #[error("the sign-in refused the {step}{detail}")]
    LoginRefused { step: &'static str, detail: String },

    /// The import produced nothing usable, which is usually the wrong file or
    /// an export that did not include the auth host.
    #[error("those cookies do not carry a Kmart session{detail}")]
    NoSession { detail: String },

    #[error("{0}")]
    Shape(String),

    #[error("{context}")]
    Decode {
        context: String,
        #[source]
        source: serde_json::Error,
    },
}

impl Error {
    pub fn decode(context: impl Into<String>, source: serde_json::Error) -> Error {
        Error::Decode {
            context: context.into(),
            source,
        }
    }

    pub fn body(&self) -> &str {
        match self {
            Error::Http(e) => e.body(),
            _ => "",
        }
    }

    /// Whether a client holding stored credentials should try renewing and go
    /// again.
    pub fn is_lapsed(&self) -> bool {
        matches!(self, Error::SessionExpired | Error::NotSignedIn)
    }

    /// Whether this is Akamai rather than Kmart.
    ///
    /// Worth asking separately from [`Error::is_lapsed`]: a challenge is not
    /// fixed by signing in again, so a client that retried on it would loop.
    pub fn is_challenge(&self) -> bool {
        matches!(self, Error::Challenged { .. })
    }
}

/// Whether a response body is a bot challenge rather than an answer.
///
/// Three forms, because the three hosts answer differently and none of them
/// uses a status that says so:
///
/// - the gateway answers JSON carrying `cpr_chlge`;
/// - the storefront answers a page whose only real content is the sensor
///   script and the container it draws into;
/// - the login host answers an Akamai "Access Denied" page, which arrives as a
///   `403` and is easy to mistake for the credentials being wrong. That one
///   matters most: it is what a password submit gets, and reporting it as a
///   refused password would send someone to check a password that was fine.
pub fn is_challenge_body(body: &str) -> bool {
    body.contains("cpr_chlge")
        || body.contains("sec-if-cpt-container")
        // Akamai HTML-escapes the punctuation in its denial page -- the link
        // reads `errors&#46;edgesuite&#46;net` -- so the host cannot be matched
        // whole. `edgesuite` survives between the entities, and pairing it
        // with the title keeps this off a page that merely says "access
        // denied" about something else.
        || (body.contains("Access Denied") && body.contains("edgesuite"))
}

impl Fault for Error {
    fn auth(&self) -> Option<AuthFault> {
        match self {
            Error::SessionExpired | Error::SessionUnrenewable => Some(AuthFault::Expired),
            Error::NotSignedIn | Error::NoSession { .. } => Some(AuthFault::Missing),
            Error::LoginRefused { .. } => Some(AuthFault::Rejected),
            Error::Http(e) => e.auth(),
            _ => None,
        }
    }

    fn is_transport(&self) -> bool {
        matches!(self, Error::Http(e) if e.is_transport())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lapsed_session_is_told_apart_from_never_having_signed_in() {
        assert!(Error::SessionExpired.is_lapsed());
        assert!(Error::NotSignedIn.is_lapsed());
        assert!(!Error::Shape("x".into()).is_lapsed());
    }

    #[test]
    fn a_challenge_is_not_a_lapsed_session() {
        // The distinction that keeps a client from looping: signing in again
        // does nothing about Akamai, and retrying does nothing either.
        let challenged = Error::Challenged {
            host: "api.kmart.co.nz".into(),
        };
        assert!(challenged.is_challenge());
        assert!(!challenged.is_lapsed());
        assert_eq!(challenged.auth(), None);
    }

    #[test]
    fn all_three_challenge_bodies_are_recognised() {
        // The gateway's, verbatim.
        assert!(is_challenge_body(r#"{"cpr_chlge":"true","t":"230881678"}"#));
        // The storefront's interstitial, which arrives as a 200.
        assert!(is_challenge_body(
            r#"<div id="sec-if-cpt-container" role="main">"#
        ));
        // The login host's, verbatim off the wire -- entities and all. The
        // escaping is the point: an earlier version of this matched on
        // `errors.edgesuite.net` and never fired, so a blocked password submit
        // was reported as a wrong password.
        assert!(is_challenge_body(
            "<HTML><HEAD>\n<TITLE>Access Denied</TITLE>\n</HEAD><BODY>\n<H1>Access Denied</H1>\n \
             You don't have permission to access \
             &#34;http&#58;&#47;&#47;auth&#46;kmart&#46;com&#46;au&#47;u&#47;login&#47;password&#63;&#34; \
             on this server.<P>\nReference&#32;&#35;18&#46;d83d517&#46;1788554069&#46;499e0ba\n<P>\
             https&#58;&#47;&#47;errors&#46;edgesuite&#46;net&#47;18&#46;d83d517&#46;1788554069&#46;499e0ba</P>"
        ));
        assert!(!is_challenge_body(r#"{"data":{"postcodeQuery":[]}}"#));
        // An Auth0 page that merely says a login was denied is not this.
        assert!(!is_challenge_body(
            r#"<span id="error-element-password">Access Denied for this user</span>"#
        ));
    }

    #[test]
    fn auth_kinds_are_distinct() {
        assert_eq!(Error::SessionExpired.auth(), Some(AuthFault::Expired));
        assert_eq!(Error::NotSignedIn.auth(), Some(AuthFault::Missing));
        assert_eq!(
            Error::LoginRefused {
                step: "password",
                detail: String::new()
            }
            .auth(),
            Some(AuthFault::Rejected)
        );
        assert_eq!(Error::Shape("x".into()).auth(), None);
    }
}
