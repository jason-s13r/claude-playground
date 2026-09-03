//! Failures while checking for or installing a release.

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Net(#[from] net_kit::Error),

    #[error("{0}")]
    Http(#[from] net_kit::HttpError),

    /// The unauthenticated GitHub rate limit, which is the failure people
    /// actually hit, and which has a specific fix.
    #[error("GitHub declined the update check ({status}). This is usually the unauthenticated rate limit; set GITHUB_TOKEN, or try again later.")]
    RateLimited { status: u16 },

    #[error("{0}")]
    NoAsset(String),

    #[error("{tag} publishes no SHA256SUMS, so the download cannot be verified; install it by hand from {url} if you trust it")]
    Unverifiable { tag: String, url: String },

    #[error("{name} does not match its published checksum\n  expected {expected}\n  got      {actual}\nrefusing to install it")]
    ChecksumMismatch {
        name: String,
        expected: String,
        actual: String,
    },

    #[error("{0}")]
    Archive(String),

    #[error("cannot write to {dir}: {source}\nthis tool installs over itself, so it needs write access to the directory it lives in. Re-run with permission to write there, or unpack the release tarball by hand.")]
    NotWritable {
        dir: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("`{0}` is not a version")]
    BadVersion(String),
}

impl Error {
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Error {
        Error::Io {
            context: context.into(),
            source,
        }
    }
}
