//! What `main` turns into an exit code.
//!
//! [`kmart_api::Error`] is the interesting half. This enum exists because the
//! app also has failures Kmart is not responsible for: an unreadable config, a
//! bad flag combination, a failed self-update.

use kmart_api::Error as Api;

pub type AppResult<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Api(#[from] Api),

    #[error(transparent)]
    Net(#[from] net_kit::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Update(#[from] build_kit::Error),

    /// A flag combination no amount of network work would fix.
    #[error("{0}")]
    Usage(String),

    /// A failure the command has already described in full.
    ///
    /// `doctor` prints a report saying exactly what is wrong; adding
    /// "kmart: something is wrong" underneath it says less than the report
    /// already did, but the exit code still has to carry.
    #[error("")]
    Reported(u8),
}

impl AppError {
    pub fn usage(message: impl Into<String>) -> AppError {
        AppError::Usage(message.into())
    }

    /// 2 is the shell's convention for misuse; 3 is an auth problem and 5 a
    /// missing store, so a wrapper can tell "sign in again" from "that product
    /// does not exist" without reading the message.
    pub fn exit_code(&self) -> u8 {
        match self {
            AppError::Usage(_) => 2,
            AppError::Reported(code) => *code,
            AppError::Api(e) => match e {
                Api::SessionExpired
                | Api::NotSignedIn
                | Api::SessionUnrenewable
                | Api::LoginRefused { .. } => 3,
                Api::NoSuchStore(_) => 5,
                Api::NoSuchProduct(_) | Api::NoSuchCategory(_) => 6,
                // Its own code, so a script driving this in a loop can back
                // off rather than hammering on through a generic failure.
                Api::RateLimited { .. } => 7,
                // Distinct from a rate limit on purpose: waiting does not
                // clear a bot challenge, so a script must not retry on it.
                Api::Challenged { .. } | Api::NoSession { .. } => 8,
                _ => 1,
            },
            _ => 1,
        }
    }

    /// Whether `main` should print anything, or the command already did.
    pub fn silent(&self) -> bool {
        matches!(self, AppError::Reported(_))
    }

    /// What kind of thing would fix this, in the library's words. The command
    /// line that does it is `cli::advice`, because only the binary knows what
    /// it is called.
    pub fn hint(&self) -> Option<&'static str> {
        let AppError::Api(api) = self else {
            return None;
        };
        match api {
            Api::Challenged { .. } => Some(
                "Kmart's bot check answers anything that is not a browser; \
                 the catalogue commands need none of this",
            ),
            Api::CartConflict => Some("the cart moved between reading it and writing it"),
            _ => None,
        }
    }
}

impl From<toml::de::Error> for AppError {
    fn from(e: toml::de::Error) -> AppError {
        AppError::Usage(format!("the config file is not valid TOML: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exit_code_tells_a_script_what_kind_of_failure_it_was() {
        assert_eq!(AppError::usage("bad flag").exit_code(), 2);
        assert_eq!(AppError::Api(Api::NotSignedIn).exit_code(), 3);
        assert_eq!(AppError::Api(Api::SessionExpired).exit_code(), 3);
        assert_eq!(
            AppError::Api(Api::NoSuchStore("Nowhere".into())).exit_code(),
            5
        );
        assert_eq!(
            AppError::Api(Api::NoSuchProduct("00000000".into())).exit_code(),
            6
        );
        assert_eq!(AppError::Api(Api::Shape("odd".into())).exit_code(), 1);
    }

    #[test]
    fn a_challenge_and_a_rate_limit_do_not_share_a_code() {
        // A script may retry a 7 after a wait. Retrying an 8 is pointless --
        // no amount of waiting produces an Akamai sensor payload.
        assert_eq!(
            AppError::Api(Api::RateLimited { retry_after: None }).exit_code(),
            7
        );
        assert_eq!(
            AppError::Api(Api::Challenged {
                host: "api.kmart.co.nz".into()
            })
            .exit_code(),
            8
        );
    }

    #[test]
    fn a_reported_failure_carries_its_code_without_printing_twice() {
        let e = AppError::Reported(1);
        assert!(e.silent());
        assert_eq!(e.exit_code(), 1);
    }
}
