//! Failures with their evidence still attached.
//!
//! Both existing CLIs decide what went wrong by formatting an `anyhow` chain
//! and matching substrings against it -- `text.contains("401")`. That survives
//! exactly until someone adds a `.context()` line above it. Here an HTTP
//! failure keeps its status code and its raw body, so a caller asks a question
//! instead of guessing at English.

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] HttpError),

    #[error("{context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{context}")]
    Keyring {
        context: String,
        #[source]
        source: keyring::Error,
    },

    #[error("{context}")]
    Decode {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    // `detail` is inline rather than a `#[source]`: `toml` reports the line
    // and the offending key there, and a message that says only "reading
    // config.toml" leaves nothing to act on.
    #[error("{context}: {detail}")]
    Toml { context: String, detail: String },

    #[error("{command} failed: {detail}")]
    Command { command: String, detail: String },

    #[error("{0}")]
    Config(String),
}

impl Error {
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Error {
        Error::Io {
            context: context.into(),
            source,
        }
    }

    pub fn keyring(context: impl Into<String>, source: keyring::Error) -> Error {
        Error::Keyring {
            context: context.into(),
            source,
        }
    }

    pub fn decode(context: impl Into<String>, source: serde_json::Error) -> Error {
        Error::Decode {
            context: context.into(),
            source,
        }
    }
}

/// What kind of authentication failure this was.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AuthFault {
    /// Nothing was presented.
    Missing,
    /// Presented and refused -- 401.
    Rejected,
    /// Accepted, but not allowed to do this -- 403.
    Forbidden,
    /// Known to have run out, from a claim or an explicit upstream code.
    Expired,
}

/// Implemented by every crate's error type, so retry logic can ask about a
/// failure without knowing which crate produced it.
pub trait Fault {
    fn auth(&self) -> Option<AuthFault>;

    /// A connection problem rather than an answer. Worth retrying; an auth
    /// failure is not.
    fn is_transport(&self) -> bool {
        false
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("{method} {url}: {source}")]
    Transport {
        method: &'static str,
        url: String,
        #[source]
        source: wreq::Error,
    },

    #[error("{method} {url}: HTTP {status}{}", detail_suffix(detail))]
    Status {
        method: &'static str,
        url: String,
        status: u16,
        detail: String,
        /// The raw upstream body, untruncated and unformatted.
        ///
        /// Some upstream signals genuinely are strings with no code beside
        /// them -- Foodstuffs answers an unbound cart with "Store is not
        /// defined". Keeping the body means that is matched once, here, at the
        /// call site that knows what it means, rather than being re-matched
        /// against a formatted error chain further up.
        body: String,
    },

    #[error("{url} did not answer with JSON: {snippet}")]
    Decode { url: String, snippet: String },
}

fn detail_suffix(detail: &str) -> String {
    if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    }
}

impl HttpError {
    pub fn status(&self) -> Option<u16> {
        match self {
            HttpError::Status { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn body(&self) -> &str {
        match self {
            HttpError::Status { body, .. } => body,
            _ => "",
        }
    }

    pub fn url(&self) -> &str {
        match self {
            HttpError::Transport { url, .. }
            | HttpError::Status { url, .. }
            | HttpError::Decode { url, .. } => url,
        }
    }
}

impl Fault for HttpError {
    fn auth(&self) -> Option<AuthFault> {
        match self.status() {
            Some(401) => Some(AuthFault::Rejected),
            Some(403) => Some(AuthFault::Forbidden),
            _ => None,
        }
    }

    fn is_transport(&self) -> bool {
        matches!(self, HttpError::Transport { .. })
    }
}

impl Fault for Error {
    fn auth(&self) -> Option<AuthFault> {
        match self {
            Error::Http(e) => e.auth(),
            _ => None,
        }
    }

    fn is_transport(&self) -> bool {
        matches!(self, Error::Http(e) if e.is_transport())
    }
}

/// Cut a body down for a message without losing that it was cut.
pub fn truncate(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: u16, body: &str) -> HttpError {
        HttpError::Status {
            method: "GET",
            url: "https://example.test/thing".into(),
            status: code,
            detail: String::new(),
            body: body.into(),
        }
    }

    #[test]
    fn classifies_auth_failures_by_code_not_by_message_text() {
        assert_eq!(status(401, "").auth(), Some(AuthFault::Rejected));
        assert_eq!(status(403, "").auth(), Some(AuthFault::Forbidden));
        assert_eq!(status(500, "").auth(), None);
        // The point of the exercise: a body that merely says "401" is not one.
        assert_eq!(status(200, "the number 401 appears here").auth(), None);
    }

    #[test]
    fn keeps_the_raw_body_for_the_one_caller_that_needs_it() {
        let e = status(400, r#"{"message":"Store is not defined"}"#);
        assert!(e.body().contains("Store is not defined"));
        assert_eq!(e.status(), Some(400));
    }

    #[test]
    fn truncation_says_it_truncated() {
        assert_eq!(truncate("short", 20), "short");
        assert_eq!(truncate("abcdefghij", 4), "abcd...");
    }
}
