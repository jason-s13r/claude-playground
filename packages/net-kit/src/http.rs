//! Building a client that is not scored as a bot, and reading answers off it.
//!
//! Not `reqwest`. The storefronts this is aimed at sit behind Cloudflare and
//! Akamai, which fingerprint the TLS handshake and the HTTP/2 settings rather
//! than the headers -- every `reqwest` TLS backend is scored as a bot, and the
//! answer is a bare 400 or a challenge page. `wreq` presents a real browser's
//! fingerprint. Do not add `http1_only()`: a browser fingerprint speaking
//! HTTP/1.1 is itself inconsistent and gets challenged.

use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use wreq_util::Profile;

use crate::error::{truncate, HttpError};

/// How a client should present itself.
///
/// There is deliberately **no `Default`**, and `profile` and `redirect` are
/// required arguments. The two vendors want opposite things: one needs a cookie
/// jar and followed redirects, the other needs no jar and *no* redirect policy,
/// so that an unexpected redirect surfaces as the bot check it is instead of
/// being quietly followed. A default here would eventually be pointed at the
/// wrong one, and the failure is silent.
pub struct ClientSpec {
    pub profile: Profile,
    pub cookies: Option<Arc<dyn wreq::cookie::CookieStore>>,
    pub redirect: wreq::redirect::Policy,
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl ClientSpec {
    pub fn new(profile: Profile, redirect: wreq::redirect::Policy) -> ClientSpec {
        ClientSpec {
            profile,
            cookies: None,
            redirect,
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
        }
    }

    pub fn with_cookies(mut self, jar: Arc<dyn wreq::cookie::CookieStore>) -> ClientSpec {
        self.cookies = Some(jar);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> ClientSpec {
        self.timeout = timeout;
        self
    }
}

pub fn build(spec: ClientSpec) -> Result<wreq::Client, wreq::Error> {
    let mut builder = wreq::Client::builder()
        .emulation(spec.profile)
        .redirect(spec.redirect)
        .timeout(spec.timeout)
        .connect_timeout(spec.connect_timeout);
    if let Some(jar) = spec.cookies {
        builder = builder.cookie_provider(jar);
    }
    builder.build()
}

/// The `User-Agent` an emulation profile sends.
///
/// Read off the profile rather than off a built client, which no longer
/// exposes the headers it was made with. Needed because at least one login
/// exchange echoes the user agent back in the *request body*, and a mismatch
/// between that and the handshake is itself a signal.
pub fn user_agent(profile: Profile) -> String {
    wreq::IntoEmulation::into_emulation(profile)
        .headers
        .get(wreq::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// How much of an upstream body to quote in an error message.
const DETAIL: usize = 300;

/// Turn a response into a decoded body, or an error that still carries the
/// status code and the raw body.
///
/// `detail` is what a caller sees; `body` is what a caller can match on. Both
/// come off the same read, because the body can only be consumed once.
pub async fn json<T: DeserializeOwned>(
    method: &'static str,
    url: &str,
    response: Result<wreq::Response, wreq::Error>,
) -> Result<T, HttpError> {
    let response = response.map_err(|source| HttpError::Transport {
        method,
        url: url.to_string(),
        source,
    })?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|source| HttpError::Transport {
            method,
            url: url.to_string(),
            source,
        })?;

    if !status.is_success() {
        return Err(HttpError::Status {
            method,
            url: url.to_string(),
            status: status.as_u16(),
            detail: truncate(&body, DETAIL),
            body,
        });
    }

    serde_json::from_str(&body).map_err(|_| HttpError::Decode {
        url: url.to_string(),
        snippet: truncate(&body, 200),
    })
}

/// The same, for an endpoint whose body is not JSON or is not wanted.
pub async fn text(
    method: &'static str,
    url: &str,
    response: Result<wreq::Response, wreq::Error>,
) -> Result<(wreq::header::HeaderMap, String), HttpError> {
    let response = response.map_err(|source| HttpError::Transport {
        method,
        url: url.to_string(),
        source,
    })?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .text()
        .await
        .map_err(|source| HttpError::Transport {
            method,
            url: url.to_string(),
            source,
        })?;

    if !status.is_success() {
        return Err(HttpError::Status {
            method,
            url: url.to_string(),
            status: status.as_u16(),
            detail: truncate(&body, DETAIL),
            body,
        });
    }
    Ok((headers, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_names_a_real_browser() {
        let ua = user_agent(Profile::Chrome137);
        assert!(ua.contains("Mozilla"), "{ua}");
        assert!(ua.contains("Chrome"), "{ua}");
        assert_ne!(ua, user_agent(Profile::Firefox139), "profiles differ");
    }

    #[test]
    fn a_client_builds_with_and_without_a_jar() {
        let bare = build(ClientSpec::new(
            Profile::Chrome137,
            wreq::redirect::Policy::none(),
        ));
        assert!(bare.is_ok());

        let jar = Arc::new(wreq::cookie::Jar::default());
        let with = build(
            ClientSpec::new(Profile::Firefox139, wreq::redirect::Policy::limited(10))
                .with_cookies(jar),
        );
        assert!(with.is_ok());
    }
}
