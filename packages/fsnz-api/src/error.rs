//! What Foodstuffs said no with.

use net_kit::{AuthFault, Fault, HttpError};

use crate::banner::Banner;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] HttpError),

    #[error(transparent)]
    Net(#[from] net_kit::Error),

    #[error("the {banner} token was not accepted")]
    Unauthorised { banner: Banner },

    /// This account's cart has no store bound to it.
    ///
    /// The signal genuinely is a string in the response body with no code
    /// beside it, so it is matched exactly once -- here, at the boundary where
    /// the raw body is still in hand -- and converted immediately. Nothing
    /// downstream re-formats and re-matches it.
    #[error("this account's cart has no store bound to it")]
    CartStoreUnbound,

    #[error("two-factor verification is required")]
    VerificationRequired {
        method: Option<String>,
        phv_token: String,
    },

    #[error("the Club Plus refresh token was refused")]
    RefreshRejected,

    #[error("{banner} set no {cookie} cookie on {url} (HTTP {status})")]
    NoToken {
        banner: Banner,
        cookie: &'static str,
        url: String,
        status: u16,
    },

    /// A Cloudflare interstitial rather than an API answer.
    ///
    /// Deliberately carries no status code: it is not an authentication
    /// failure, and treating it as one would spend a renewal on a challenge
    /// that a renewal cannot clear.
    #[error("{host} answered with a bot check rather than an API response")]
    Challenged { host: String },

    #[error("{context}")]
    Decode {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("{0}")]
    Shape(String),
}

impl Error {
    pub fn decode(context: impl Into<String>, source: serde_json::Error) -> Error {
        Error::Decode {
            context: context.into(),
            source,
        }
    }

    /// The raw upstream body, when there is one to match on.
    pub fn body(&self) -> &str {
        match self {
            Error::Http(e) => e.body(),
            _ => "",
        }
    }
}

impl Fault for Error {
    fn auth(&self) -> Option<AuthFault> {
        match self {
            Error::Unauthorised { .. } => Some(AuthFault::Rejected),
            Error::RefreshRejected => Some(AuthFault::Expired),
            Error::Http(e) => e.auth(),
            // Not an auth failure: a renewal cannot clear a bot check, and
            // spending one on it wastes a login.
            Error::Challenged { .. } => None,
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

    fn status(code: u16, body: &str) -> Error {
        Error::Http(HttpError::Status {
            method: "GET",
            url: "https://api-prod.newworld.co.nz/v1/edge/cart".into(),
            status: code,
            detail: String::new(),
            body: body.into(),
        })
    }

    #[test]
    fn a_bot_check_is_not_an_auth_failure() {
        // Renewing a session cannot clear a Cloudflare interstitial, and
        // treating it as auth would spend a login on it.
        let e = Error::Challenged {
            host: "www.newworld.co.nz".into(),
        };
        assert_eq!(e.auth(), None);
    }

    #[test]
    fn auth_failures_are_read_off_the_status_code() {
        assert_eq!(status(401, "").auth(), Some(AuthFault::Rejected));
        assert_eq!(status(403, "").auth(), Some(AuthFault::Forbidden));
        assert_eq!(status(400, "Store is not defined").auth(), None);
    }

    #[test]
    fn the_raw_body_survives_for_the_one_caller_that_matches_on_it() {
        let e = status(400, r#"{"message":"Store is not defined"}"#);
        assert!(e.body().contains("Store is not defined"));
    }
}
