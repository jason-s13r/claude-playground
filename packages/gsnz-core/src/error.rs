//! What went wrong, in terms a person can act on.
//!
//! Both existing CLIs classify failures by formatting an `anyhow` chain and
//! matching substrings against it -- `text.contains("401")`, `text.contains("has
//! expired")`. That works until someone adds a `.context()` line. Here the kind
//! is a variant, decided once at the boundary where the evidence exists.

use crate::retailer::RetailerId;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{retailer} does not support {feature}")]
    Unsupported {
        retailer: RetailerId,
        feature: &'static str,
        hint: Option<String>,
    },

    #[error("not signed in to {retailer}: run `gsnz -b {} auth login`", retailer.short())]
    NeedsLogin { retailer: RetailerId },

    #[error("the {retailer} session has expired")]
    SessionExpired {
        retailer: RetailerId,
        /// Whether `auth refresh` can fix it without a full login.
        renewable: bool,
    },

    /// The credentials were wrong, or the shop would not take them.
    ///
    /// Distinct from [`Error::SessionExpired`] on purpose: that one means a
    /// session went stale and can often be renewed, and telling someone to run
    /// `auth refresh` when they mistyped a password wastes their time on a
    /// command that cannot help.
    #[error("{retailer} refused the sign-in: {detail}")]
    LoginRefused {
        retailer: RetailerId,
        detail: String,
    },

    /// The account's cart has no store bound to it, server-side.
    ///
    /// Not the same as [`Error::NoStore`], which is a local setting this tool
    /// owns. Telling someone to run `store set` here sends them to a command
    /// that writes a config file and changes nothing about the cart.
    #[error("the {retailer} cart is not bound to a store")]
    CartUnbound { retailer: RetailerId },

    #[error("no {retailer} store selected: run `gsnz -b {} store set <id or name>`", retailer.short())]
    NoStore { retailer: RetailerId },

    #[error("{retailer}: {message}")]
    Upstream {
        retailer: RetailerId,
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn unsupported(retailer: RetailerId, feature: &'static str) -> Error {
        Error::Unsupported {
            retailer,
            feature,
            hint: None,
        }
    }

    /// The same refusal, saying what to do instead. Worth the extra call: a
    /// gap the user can route around is not the same as a dead end.
    pub fn unsupported_hint(
        retailer: RetailerId,
        feature: &'static str,
        hint: impl Into<String>,
    ) -> Error {
        Error::Unsupported {
            retailer,
            feature,
            hint: Some(hint.into()),
        }
    }

    /// The extra line printed under the message, when there is something more
    /// useful to say than the failure itself.
    pub fn hint(&self) -> Option<&str> {
        match self {
            Error::Unsupported { hint, .. } => hint.as_deref(),
            Error::SessionExpired {
                retailer,
                renewable,
            } => Some(if *renewable {
                "run `gsnz auth refresh`"
            } else {
                match retailer {
                    RetailerId::Woolworths => {
                        "a Woolworths session cannot be renewed from a cookie; run `gsnz -b ww auth login`"
                    }
                    _ => "run `gsnz auth login`",
                }
            }),
            // `store set` writes a config file; it does not touch the cart.
            Error::CartUnbound { .. } => Some(
                "this is the account's cart, not a local setting: open the shop's website \
                 once and choose a store for it",
            ),
            _ => None,
        }
    }

    /// Distinct exit codes so a script can tell "log in again" from "this shop
    /// cannot do that" without parsing stderr. Neither existing CLI offers
    /// this; anything wrapping `gsnz` will want it.
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::NeedsLogin { .. }
            | Error::SessionExpired { .. }
            | Error::LoginRefused { .. } => 3,
            Error::Unsupported { .. } => 4,
            Error::NoStore { .. } | Error::CartUnbound { .. } => 5,
            _ => 1,
        }
    }

    /// The retailer this is about, when it is about one.
    pub fn retailer(&self) -> Option<RetailerId> {
        match self {
            Error::Unsupported { retailer, .. }
            | Error::LoginRefused { retailer, .. }
            | Error::NeedsLogin { retailer }
            | Error::SessionExpired { retailer, .. }
            | Error::CartUnbound { retailer }
            | Error::NoStore { retailer }
            | Error::Upstream { retailer, .. } => Some(*retailer),
            Error::Other(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unbound_cart_is_not_confused_with_an_unselected_store() {
        // Both are "there is no store", and only one of them is fixed by
        // `store set` -- which writes a config file and leaves the cart alone.
        let unbound = Error::CartUnbound {
            retailer: RetailerId::PaknSave,
        };
        assert_eq!(unbound.exit_code(), 5);
        assert!(!unbound.to_string().contains("store set"));
        assert!(unbound.hint().unwrap().contains("website"));

        let unselected = Error::NoStore {
            retailer: RetailerId::PaknSave,
        };
        assert!(unselected.to_string().contains("store set"));
    }

    #[test]
    fn a_refused_sign_in_does_not_suggest_renewing_anything() {
        // Telling someone to run `auth refresh` when they mistyped a password
        // sends them to a command that cannot possibly help.
        let refused = Error::LoginRefused {
            retailer: RetailerId::Woolworths,
            detail: "the password was not accepted".into(),
        };
        assert_eq!(refused.exit_code(), 3);
        assert_eq!(refused.hint(), None);
        assert!(refused.to_string().contains("refused the sign-in"));
    }

    #[test]
    fn auth_failures_share_an_exit_code_distinct_from_unsupported() {
        let login = Error::NeedsLogin {
            retailer: RetailerId::Woolworths,
        };
        let expired = Error::SessionExpired {
            retailer: RetailerId::NewWorld,
            renewable: true,
        };
        let missing = Error::unsupported(RetailerId::Woolworths, "order detail");
        assert_eq!(login.exit_code(), 3);
        assert_eq!(expired.exit_code(), 3);
        assert_eq!(missing.exit_code(), 4);
        assert_eq!(Error::Other("boom".into()).exit_code(), 1);
    }

    #[test]
    fn messages_name_the_command_that_fixes_them() {
        let e = Error::NeedsLogin {
            retailer: RetailerId::Woolworths,
        };
        assert!(e.to_string().contains("gsnz -b ww auth login"), "{e}");
        let e = Error::NoStore {
            retailer: RetailerId::PaknSave,
        };
        assert!(e.to_string().contains("gsnz -b pns store set"), "{e}");
    }

    #[test]
    fn a_woolworths_session_is_not_advertised_as_renewable() {
        let e = Error::SessionExpired {
            retailer: RetailerId::Woolworths,
            renewable: false,
        };
        assert!(e.hint().unwrap().contains("auth login"));
        let e = Error::SessionExpired {
            retailer: RetailerId::NewWorld,
            renewable: true,
        };
        assert!(e.hint().unwrap().contains("auth refresh"));
    }
}
