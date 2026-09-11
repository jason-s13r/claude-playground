//! What the Briscoe Group storefronts said no with.
//!
//! Two things here are particular to this pair of sites. Signing in is
//! **captcha-gated and refreshing is not**, so "you need a browser" and "your
//! session lapsed" are different failures with different fixes and must not
//! collapse into one. And the storefront is Magento, which reports business
//! failures as a `200` carrying `errors[]` -- so a refused coupon and a
//! transport error arrive by completely different routes and both have to end
//! up here.

use net_kit::{AuthFault, Fault, HttpError};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] HttpError),

    #[error(transparent)]
    Net(#[from] net_kit::Error),

    /// The site asked this client to slow down.
    ///
    /// Worth its own variant rather than an anonymous 429: it is the one
    /// failure where the right response is to stop, not to retry harder, and a
    /// person seeing "HTTP 429" often does the opposite.
    #[error("{banner} is rate-limiting this client{}", match .retry_after {
        Some(secs) => format!("; it asked for {secs}s before the next request"),
        None => String::new(),
    })]
    RateLimited {
        banner: &'static str,
        retry_after: Option<u64>,
    },

    /// Magento answered `200` with an `errors[]` entry. The operation name is
    /// carried because the message alone -- "Internal server error" is a
    /// favourite -- rarely says which call produced it.
    #[error("{operation} was refused: {message}")]
    Graphql {
        operation: &'static str,
        message: String,
    },

    /// Gigya answered with a non-zero `errorCode`. Its `errorDetails` is often
    /// the only useful half, so both are carried.
    #[error("{method} was refused by Gigya: {message}{}", match .detail {
        Some(d) => format!(" ({d})"),
        None => String::new(),
    })]
    Gigya {
        method: &'static str,
        code: i64,
        message: String,
        detail: Option<String>,
    },

    /// Gigya refused the login for want of a captcha token.
    ///
    /// Its own variant because it is not a credential problem and re-entering
    /// a password will never fix it: `accounts.login` validates the captcha
    /// *before* it looks at the password, so this arrives just as readily with
    /// a correct one. The fix is a browser, and only the app knows how to
    /// offer that.
    #[error("signing in to {banner} needs a captcha token that only a browser can mint")]
    CaptchaRequired { banner: &'static str },

    /// The Magento customer token has expired or was never held. Recoverable
    /// without a browser: see [`crate::auth`].
    #[error("the {banner} session has expired")]
    SessionExpired { banner: &'static str },

    #[error("not signed in to {banner}")]
    NotSignedIn { banner: &'static str },

    /// The stored Gigya `login_token` was refused, which is the one auth
    /// failure a refresh cannot fix.
    #[error("the stored {banner} login has lapsed and a new sign-in is needed")]
    LoginLapsed { banner: &'static str },

    #[error("no product called {0}")]
    NoSuchProduct(String),

    #[error("no store called {0}")]
    NoSuchStore(String),

    /// The answer parsed but did not contain what was asked for. Named apart
    /// from a decode failure because the fix differs: this is the schema having
    /// moved, not the JSON being malformed.
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

    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Error::RateLimited { .. })
    }

    /// Whether a client holding a stored Gigya login should mint a new Magento
    /// token and try again. Deliberately excludes [`Error::LoginLapsed`]: that
    /// one has already tried and failed, and retrying it loops.
    pub fn is_lapsed(&self) -> bool {
        matches!(
            self,
            Error::SessionExpired { .. } | Error::NotSignedIn { .. }
        )
    }

    /// Whether the only way forward is a browser.
    pub fn needs_browser(&self) -> bool {
        matches!(
            self,
            Error::CaptchaRequired { .. } | Error::LoginLapsed { .. }
        )
    }
}

impl Fault for Error {
    fn auth(&self) -> Option<AuthFault> {
        match self {
            Error::SessionExpired { .. } => Some(AuthFault::Expired),
            Error::NotSignedIn { .. } => Some(AuthFault::Missing),
            Error::LoginLapsed { .. } | Error::CaptchaRequired { .. } => Some(AuthFault::Rejected),
            Error::Http(e) => e.auth(),
            _ => None,
        }
    }

    fn is_transport(&self) -> bool {
        matches!(self, Error::Http(e) if e.is_transport())
    }
}

/// Gigya's error codes, for the two that need telling apart.
///
/// A missing captcha is deliberately not among them: it arrives as `400006`,
/// "Invalid parameter value", which is also what every other malformed
/// parameter arrives as. The distinguishing part is in `errorDetails`, so
/// [`crate::auth`] reads that instead.
///
/// Wrong loginID or password. The one Gigya failure that is the person's fault.
pub const GIGYA_INVALID_CREDENTIALS: i64 = 403_042;
/// The `login_token` is no longer a session.
pub const GIGYA_INVALID_SESSION: i64 = 403_005;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_captcha_is_not_a_credential_problem() {
        // The distinction is the whole point: one is fixed by a browser, the
        // other by a password, and confusing them sends someone to retype a
        // password that was never wrong.
        let captcha = Error::CaptchaRequired { banner: "Briscoes" };
        assert!(captcha.needs_browser());
        assert!(!captcha.is_lapsed());
        assert_eq!(captcha.auth(), Some(AuthFault::Rejected));
    }

    #[test]
    fn an_expired_token_is_refreshable_and_a_lapsed_login_is_not() {
        // `is_lapsed` gates an automatic retry, so a lapsed login answering
        // true would loop: refresh, fail, refresh again.
        let expired = Error::SessionExpired {
            banner: "Rebel Sport",
        };
        assert!(expired.is_lapsed());
        assert!(!expired.needs_browser());

        let lapsed = Error::LoginLapsed {
            banner: "Rebel Sport",
        };
        assert!(!lapsed.is_lapsed());
        assert!(lapsed.needs_browser());
    }

    #[test]
    fn a_rate_limit_is_named_rather_than_left_as_a_status_code() {
        let limited = Error::RateLimited {
            banner: "Briscoes",
            retry_after: Some(30),
        };
        assert!(limited.is_rate_limited());
        assert!(limited.to_string().contains("rate-limiting"), "{limited}");
        assert!(limited.to_string().contains("30s"), "{limited}");
        assert_eq!(limited.auth(), None);
    }

    #[test]
    fn the_recorded_gigya_codes_are_the_ones_observed() {
        assert_eq!(GIGYA_INVALID_CREDENTIALS, 403_042);
        assert_eq!(GIGYA_INVALID_SESSION, 403_005);
    }

    #[test]
    fn a_graphql_refusal_names_the_operation_it_came_from() {
        // Magento's favourite message is "Internal server error", which on its
        // own identifies nothing.
        let e = Error::Graphql {
            operation: "loginGigya",
            message: "Internal server error".into(),
        };
        assert!(e.to_string().contains("loginGigya"), "{e}");
    }
}
