//! What `main` turns into an exit code.
//!
//! [`twlnz_api::Error`] is the interesting half. This enum exists because the
//! app also has failures the retailer is not responsible for: an unreadable
//! config, a bad flag combination, a failed self-update.

use twlnz_api::Error as Api;

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
    /// "twlnz: something is wrong" underneath it says less than the report
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
                Api::SessionExpired | Api::NotSignedIn | Api::LoginRefused { .. } => 3,
                Api::NoSuchStore(_) => 5,
                // Its own code, so a script driving this in a loop can back off
                // rather than hammering on through a generic failure.
                Api::RateLimited { .. } => 7,
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
            Api::TokenExpired { .. } => {
                Some("the page token expired; the product page has to be read again")
            }
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
        assert_eq!(AppError::Api(Api::Shape("odd".into())).exit_code(), 1);
    }

    #[test]
    fn a_rate_limit_has_its_own_code_so_a_script_can_back_off() {
        assert_eq!(
            AppError::Api(Api::RateLimited { retry_after: None }).exit_code(),
            7
        );
    }

    #[test]
    fn a_reported_failure_carries_its_code_without_printing_twice() {
        let e = AppError::Reported(1);
        assert!(e.silent());
        assert_eq!(e.exit_code(), 1);
    }
}
