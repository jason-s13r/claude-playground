//! What Woolworths said no with.
//!
//! The interesting case is that GraphQL answers 200 with an `errors` array
//! rather than an HTTP status, so "not signed in" arrives as an extension code
//! on a successful response. Reading that code is what replaces the original's
//! `format!("{e:#}").contains("has expired")`.

use net_kit::{AuthFault, Fault, HttpError};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] HttpError),

    #[error(transparent)]
    Net(#[from] net_kit::Error),

    /// `AUTH_NOT_AUTHENTICATED` on a 200, or a 401 naming `session_expired`.
    #[error("the Woolworths session has expired")]
    SessionExpired,

    #[error("not signed in to Woolworths")]
    NotSignedIn,

    /// A Woolworths session cannot be renewed from what is stored: the cookie
    /// is encrypted and only the site can mint one. Renewal means a whole
    /// login, which needs a password.
    #[error("the stored Woolworths session cannot be renewed without a password")]
    SessionUnrenewable,

    #[error("{operation} failed: {message}")]
    Graphql {
        operation: &'static str,
        message: String,
    },

    #[error("the sign-in page refused the {step}{detail}")]
    LoginRefused { step: &'static str, detail: String },

    /// The login flow ran but produced no session cookie, which usually means a
    /// step was added that this cannot follow.
    #[error("signing in did not produce a session{detail}")]
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

    /// Whether a client holding a password should try signing in again.
    pub fn is_lapsed(&self) -> bool {
        matches!(self, Error::SessionExpired | Error::NotSignedIn)
    }
}

impl Fault for Error {
    fn auth(&self) -> Option<AuthFault> {
        match self {
            Error::SessionExpired => Some(AuthFault::Expired),
            Error::NotSignedIn => Some(AuthFault::Missing),
            Error::SessionUnrenewable => Some(AuthFault::Expired),
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
        // Both need an account, but only one is worth retrying automatically
        // and they suggest different things to the user.
        assert!(Error::SessionExpired.is_lapsed());
        assert!(Error::NotSignedIn.is_lapsed());
        assert!(!Error::Shape("x".into()).is_lapsed());
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
