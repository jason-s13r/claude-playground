//! What the API is called with: a guest token for browsing, or an account
//! session for anything belonging to a person.
//!
//! Woolworths authorises the GraphQL endpoint entirely with cookies. Two kinds
//! matter:
//!
//! - `__guest__token`, a JWT the storefront hands to anyone who asks for the
//!   home page. Enough to search, browse and read store lists.
//! - `__session__0` / `__session__1`, an encrypted session cookie split across
//!   values because it is too big for one. Only the site's own server can mint
//!   or read it, so [`crate::auth`] obtains it by walking the login flow and
//!   this module just keeps hold of it.

use std::collections::BTreeMap;
use std::time::Duration;

use net_kit::{wreq, Paths, Secrets};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const GUEST_COOKIE: &str = "__guest__token";

/// The account session cookie, split across `__session__0`, `__session__1` and
/// so on. The site rebuilds one value by concatenating them in index order.
pub const SESSION_COOKIE_PREFIX: &str = "__session__";

/// Where a stored session is filed in the credential store.
pub const ACCOUNT: &str = "session";

/// A guest token with no readable `exp` is assumed good for this long. Short,
/// because minting another is one cheap request.
const ASSUMED_GUEST_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// Re-mint this long before a token actually expires, so one does not lapse
/// midway through a command that makes several calls.
const EXPIRY_MARGIN: Duration = Duration::from_secs(60);

/// The cookies one request is made with.
#[derive(Clone, Default)]
pub struct Session {
    cookies: BTreeMap<String, String>,
    /// Whether these cookies speak for an account rather than a guest.
    pub account: bool,
}

/// Names only. The values are credentials.
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("cookies", &self.cookies.keys().collect::<Vec<_>>())
            .field("account", &self.account)
            .finish()
    }
}

impl Session {
    pub fn guest(token: &str) -> Session {
        Session {
            cookies: [(GUEST_COOKIE.to_string(), token.to_string())]
                .into_iter()
                .collect(),
            account: false,
        }
    }

    pub fn from_cookies(cookies: BTreeMap<String, String>) -> Session {
        let account = cookies.keys().any(|k| k.starts_with(SESSION_COOKIE_PREFIX));
        Session { cookies, account }
    }

    pub fn cookies(&self) -> BTreeMap<String, String> {
        self.cookies.clone()
    }

    /// The `Cookie` header value, or `None` when there is nothing to send.
    pub fn header(&self) -> Option<String> {
        if self.cookies.is_empty() {
            return None;
        }
        Some(
            self.cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

/// A stored account session.
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredSession {
    /// The email it was obtained for, so a status command can name it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub cookies: BTreeMap<String, String>,
    /// When the login happened. There is no readable expiry on an encrypted
    /// session cookie, so age is the only thing that can be reported.
    #[serde(default)]
    pub obtained_at: u64,
}

impl std::fmt::Debug for StoredSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredSession")
            .field("email", &self.email)
            .field("cookies", &self.cookies.keys().collect::<Vec<_>>())
            .field("obtained_at", &self.obtained_at)
            .finish()
    }
}

impl StoredSession {
    pub fn load(secrets: &Secrets) -> Result<Option<StoredSession>> {
        let Some(text) = secrets.get(ACCOUNT)? else {
            return Ok(None);
        };
        // A session written by an older build, or corrupted, is worth
        // discarding rather than failing every command until it is removed by
        // hand.
        Ok(serde_json::from_str(&text).ok())
    }

    pub fn save(&self, secrets: &Secrets) -> Result<()> {
        let text =
            serde_json::to_string(self).map_err(|e| Error::decode("serialising the session", e))?;
        Ok(secrets.set(ACCOUNT, &text)?)
    }

    pub fn clear(secrets: &Secrets) -> Result<bool> {
        Ok(secrets.delete(ACCOUNT)?)
    }

    pub fn session(&self) -> Session {
        Session::from_cookies(self.cookies.clone())
    }
}

#[derive(Serialize, Deserialize)]
struct CachedGuest {
    token: String,
    expires_at: u64,
}

fn guest_file(paths: &Paths) -> std::path::PathBuf {
    paths.state_file("guest-token.json")
}

fn cached_guest(paths: &Paths) -> Option<String> {
    let cached: CachedGuest = net_kit::config::load_json_cache(&guest_file(paths))?;
    (cached.expires_at > net_kit::jwt::now_secs() + EXPIRY_MARGIN.as_secs()).then_some(cached.token)
}

fn cache_guest(paths: &Paths, token: &str) {
    let expires_at = net_kit::jwt::expiry_ms(token)
        .map(|ms| ms / 1000)
        .unwrap_or_else(|| net_kit::jwt::now_secs() + ASSUMED_GUEST_LIFETIME.as_secs());
    net_kit::config::save_json_cache(
        &guest_file(paths),
        &CachedGuest {
            token: token.to_string(),
            expires_at,
        },
    );
}

pub fn clear_guest(paths: &Paths) -> bool {
    std::fs::remove_file(guest_file(paths)).is_ok()
}

/// A session for the guest-scoped endpoints: the cached token, or a fresh one
/// from the storefront.
pub async fn guest(http: &wreq::Client, origin: &str, paths: &Paths) -> Result<Session> {
    if let Some(token) = cached_guest(paths) {
        return Ok(Session::guest(&token));
    }
    let token = mint_guest(http, origin).await?;
    cache_guest(paths, &token);
    Ok(Session::guest(&token))
}

/// Ask the storefront for a guest token, ignoring whatever page it serves.
///
/// No headers are set: the emulation already sends what a real browser sends
/// for a navigation, and overriding part of that set is how it stops being
/// self-consistent.
pub async fn mint_guest(http: &wreq::Client, origin: &str) -> Result<String> {
    let url = format!("{origin}/");
    let sent = http.get(&url).send().await;
    let (headers, _) = net_kit::http::text("GET", &url, sent).await?;
    net_kit::cookies::set_cookies(&headers)
        .remove(GUEST_COOKIE)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            Error::Shape(format!(
                "{url} set no {GUEST_COOKIE} cookie, so there is nothing to call the API with. \
                 The storefront may be serving a bot check."
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn jwt(exp: u64) -> String {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::json!({ "exp": exp }).to_string());
        format!("header.{payload}.signature")
    }

    #[test]
    fn cookie_headers_are_rebuilt_in_full_and_in_index_order() {
        let s = Session::from_cookies(
            [
                ("__session__1".to_string(), "b".to_string()),
                ("__session__0".to_string(), "a".to_string()),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(
            s.header().as_deref(),
            Some("__session__0=a; __session__1=b")
        );
        assert!(s.account, "session cookies mean an account");
    }

    #[test]
    fn a_guest_token_alone_is_not_an_account() {
        let s = Session::guest("t");
        assert_eq!(s.header().as_deref(), Some("__guest__token=t"));
        assert!(!s.account);
        assert!(Session::default().header().is_none());
    }

    #[test]
    fn a_session_prints_its_cookie_names_but_never_their_values() {
        let s = Session::from_cookies(
            [("__session__0".to_string(), "secret-value".to_string())]
                .into_iter()
                .collect(),
        );
        let text = format!("{s:?}");
        assert!(text.contains("__session__0"));
        assert!(!text.contains("secret-value"), "{text}");
    }

    #[test]
    fn a_stored_session_round_trips_and_a_corrupt_one_reads_as_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let secrets = Secrets::new("wwnz-api-test", net_kit::Backend::File, dir.path());
        assert!(StoredSession::load(&secrets).unwrap().is_none());

        StoredSession {
            email: Some("shopper@example.test".into()),
            cookies: [("__session__0".to_string(), "a".to_string())]
                .into_iter()
                .collect(),
            obtained_at: 1_756_512_000,
        }
        .save(&secrets)
        .unwrap();
        assert!(
            StoredSession::load(&secrets)
                .unwrap()
                .unwrap()
                .session()
                .account
        );

        secrets.set(ACCOUNT, "{ truncated").unwrap();
        assert!(StoredSession::load(&secrets).unwrap().is_none());
    }

    #[test]
    fn a_guest_token_is_cached_until_its_margin() {
        let dir = tempfile::TempDir::new().unwrap();
        let paths = Paths::new(dir.path().join("cfg"), dir.path().join("state"));
        assert!(cached_guest(&paths).is_none());

        cache_guest(&paths, &jwt(net_kit::jwt::now_secs() + 3600));
        assert!(cached_guest(&paths).is_some());

        // Inside the margin: treated as gone, so a command making several calls
        // does not have one lapse midway.
        cache_guest(&paths, &jwt(net_kit::jwt::now_secs() + 30));
        assert!(cached_guest(&paths).is_none());
    }

    #[test]
    fn a_token_with_no_readable_expiry_is_still_cached_briefly() {
        let dir = tempfile::TempDir::new().unwrap();
        let paths = Paths::new(dir.path().join("cfg"), dir.path().join("state"));
        cache_guest(&paths, "not-a-jwt");
        assert_eq!(cached_guest(&paths).as_deref(), Some("not-a-jwt"));
        assert!(clear_guest(&paths));
        assert!(cached_guest(&paths).is_none());
    }
}
