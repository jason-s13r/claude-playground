//! What `main` turns into an exit code.
//!
//! [`gsnz_core::Error`] is the interesting half -- it is what an adapter
//! produces and what carries the exit codes a script cares about. This enum
//! exists only because the app also has failures no retailer is responsible
//! for: an unreadable config, a bad flag combination, a failed self-update.

use gsnz_core::Error as Domain;

pub type AppResult<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] Domain),

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
    /// "fsnz: something is wrong" underneath it says less than the report
    /// already did, but the exit code still has to carry.
    #[error("")]
    Reported(u8),
}

impl AppError {
    pub fn usage(message: impl Into<String>) -> AppError {
        AppError::Usage(message.into())
    }

    /// 2 is the shell's convention for misuse; 3, 4 and 5 come from the domain
    /// so a wrapper can tell "log in again" from "this shop cannot do that".
    pub fn exit_code(&self) -> u8 {
        match self {
            AppError::Domain(e) => e.exit_code(),
            AppError::Usage(_) => 2,
            AppError::Reported(code) => *code,
            _ => 1,
        }
    }

    /// Whether `main` should print anything, or the command already did.
    pub fn silent(&self) -> bool {
        matches!(self, AppError::Reported(_))
    }

    pub fn hint(&self) -> Option<&str> {
        match self {
            AppError::Domain(e) => e.hint(),
            _ => None,
        }
    }
}

impl From<toml::de::Error> for AppError {
    fn from(e: toml::de::Error) -> AppError {
        AppError::Usage(format!("the config file is not valid TOML: {e}"))
    }
}
