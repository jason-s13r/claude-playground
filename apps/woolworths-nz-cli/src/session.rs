//! What the API is called with: a guest token for browsing, or an account
//! session for anything belonging to a person.
//!
//! Woolworths authorises the GraphQL endpoint entirely with cookies. Two kinds
//! matter:
//!
//! - `__guest__token`, a JWT the storefront hands to anyone who asks for the
//!   home page. It is enough to search, browse and read store lists.
//! - `__session__0` / `__session__1`, an encrypted session cookie split across
//!   two values because it is too big for one. Only the site's own server can
//!   mint or read it, so [`crate::auth`] obtains it by walking the login flow
//!   and this module just keeps hold of it.
//!
//! A guest token is cached; a session is a credential and goes to the secret
//! store.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;

use crate::config::Paths;
use crate::secrets::Secrets;

/// The browser this client presents as, at every layer.
///
/// Akamai scores the TLS handshake, the HTTP/2 settings and the headers
/// together, so they have to agree. `wreq` derives all three from this one
/// value -- including the `User-Agent` -- which is why nothing here sets a
/// user agent by hand: a header naming a different Firefox than the handshake
/// is exactly the inconsistency being watched for.
pub const EMULATION: wreq_util::Profile = wreq_util::Profile::Firefox139;

pub const GUEST_COOKIE: &str = "__guest__token";

/// The account session cookie, split across `__session__0`, `__session__1` and
/// so on. The site rebuilds one value by concatenating them in index order.
pub const SESSION_COOKIE_PREFIX: &str = "__session__";

/// Where a stored session is filed in the secret store.
const SESSION_ACCOUNT: &str = "session";

/// A guest token with no readable `exp` is assumed good for this long. Short
/// enough that a stale one is re-minted quickly, since minting is one cheap
/// request.
const ASSUMED_GUEST_LIFETIME: u64 = 15 * 60;

/// Re-mint a guest token this long before it actually expires, so a token does
/// not lapse midway through a command that makes several calls.
const EXPIRY_MARGIN: u64 = 60;

/// The cookies one request is made with.
#[derive(Clone, Debug, Default)]
pub struct Session {
    cookies: BTreeMap<String, String>,
    /// Whether these cookies speak for an account rather than a guest.
    pub account: bool,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredSession {
    /// The email it was obtained for, so `wwnz auth status` can name it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub cookies: BTreeMap<String, String>,
    /// When the login happened. There is no readable expiry on an encrypted
    /// session cookie, so age is the only thing that can be reported.
    #[serde(default)]
    pub obtained_at: u64,
}

impl StoredSession {
    pub fn load(secrets: &Secrets) -> Result<Option<StoredSession>> {
        let Some(text) = secrets.get(SESSION_ACCOUNT)? else {
            return Ok(None);
        };
        // A session written by an older build, or corrupted, is worth
        // discarding rather than failing every command until it is removed by
        // hand.
        Ok(serde_json::from_str(&text).ok())
    }

    pub fn save(&self, secrets: &Secrets) -> Result<()> {
        let text = serde_json::to_string(self).context("serialising the session")?;
        secrets.set(SESSION_ACCOUNT, &text)
    }

    pub fn clear(secrets: &Secrets) -> Result<bool> {
        secrets.delete(SESSION_ACCOUNT)
    }

    pub fn session(&self) -> Session {
        Session::from_cookies(self.cookies.clone())
    }
}

/// The cached guest token.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedGuest {
    token: String,
    expires_at: u64,
}

/// Read the cached guest token, if there is one that has not lapsed.
fn cached_guest(paths: &Paths) -> Option<String> {
    let text = fs::read_to_string(paths.guest_token_file()).ok()?;
    let cached: CachedGuest = serde_json::from_str(&text).ok()?;
    (cached.expires_at > now() + EXPIRY_MARGIN).then_some(cached.token)
}

fn cache_guest(paths: &Paths, token: &str) {
    let cached = CachedGuest {
        token: token.to_string(),
        expires_at: jwt_expiry(token).unwrap_or_else(|| now() + ASSUMED_GUEST_LIFETIME),
    };
    // A token that cannot be cached still works for this command; the next one
    // just mints another.
    let Ok(text) = serde_json::to_string(&cached) else {
        return;
    };
    if fs::create_dir_all(&paths.state_dir).is_ok() {
        let file = paths.guest_token_file();
        if fs::write(&file, text).is_ok() {
            crate::config::restrict(&file);
        }
    }
}

pub fn clear_guest(paths: &Paths) -> bool {
    fs::remove_file(paths.guest_token_file()).is_ok()
}

/// A session for the guest-scoped endpoints: the cached token, or a fresh one
/// from the storefront.
///
/// `origin` is the storefront, which sets the token as a cookie on any page
/// load. Nothing but a well-formed browser request is needed.
pub async fn guest(http: &wreq::Client, origin: &str, paths: &Paths) -> Result<Session> {
    if let Some(token) = env_guest() {
        return Ok(Session::guest(&token));
    }
    if let Some(token) = cached_guest(paths) {
        return Ok(Session::guest(&token));
    }
    let token = mint_guest(http, origin).await?;
    cache_guest(paths, &token);
    Ok(Session::guest(&token))
}

/// Ask the storefront for a guest token, ignoring whatever page it serves.
pub async fn mint_guest(http: &wreq::Client, origin: &str) -> Result<String> {
    let url = format!("{origin}/");
    // Headers come from the emulation; see [`EMULATION`].
    let res = http
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;

    let status = res.status();
    let cookies = set_cookies(res.headers());
    cookies.get(GUEST_COOKIE).cloned().with_context(|| {
        format!(
            "{url} returned {status} but set no {GUEST_COOKIE} cookie, so there is \
             nothing to call the API with. The storefront may be serving a bot check; \
             pass a token with WWNZ_GUEST_TOKEN if you can obtain one from a browser."
        )
    })
}

fn env_guest() -> Option<String> {
    std::env::var("WWNZ_GUEST_TOKEN")
        .ok()
        .filter(|v| !v.trim().is_empty())
}

/// Every cookie a response set, by name.
pub fn set_cookies(headers: &wreq::header::HeaderMap) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for value in headers.get_all(wreq::header::SET_COOKIE) {
        let Ok(text) = value.to_str() else { continue };
        // `name=value; Path=/; HttpOnly` -- everything after the first `;` is
        // attributes, which are of no interest to a client that keeps cookies
        // for the length of one command.
        let pair = text.split(';').next().unwrap_or_default();
        if let Some((name, value)) = pair.split_once('=') {
            let name = name.trim();
            if !name.is_empty() {
                out.insert(name.to_string(), value.trim().to_string());
            }
        }
    }
    out
}

/// The `exp` claim of a JWT, in epoch seconds.
///
/// The signature is not checked: this is a token being carried from the issuer
/// straight back to it, and the only question here is when to stop reusing it.
pub fn jwt_expiry(token: &str) -> Option<u64> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("exp")?.as_u64()
}

pub fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
    fn cookie_headers_are_rebuilt_in_full() {
        let s = Session::from_cookies(
            [
                ("__session__0".to_string(), "a".to_string()),
                ("__session__1".to_string(), "b".to_string()),
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
    fn set_cookie_parsing_keeps_the_value_and_drops_the_attributes() {
        let mut h = wreq::header::HeaderMap::new();
        h.append(
            wreq::header::SET_COOKIE,
            "__guest__token=abc.def; Path=/; HttpOnly; SameSite=Lax"
                .parse()
                .unwrap(),
        );
        h.append(wreq::header::SET_COOKIE, "other=1; Path=/".parse().unwrap());
        let cookies = set_cookies(&h);
        assert_eq!(
            cookies.get(GUEST_COOKIE).map(String::as_str),
            Some("abc.def")
        );
        assert_eq!(cookies.get("other").map(String::as_str), Some("1"));
    }

    #[test]
    fn jwt_expiry_is_read_without_verifying_anything() {
        assert_eq!(jwt_expiry(&jwt(1_800_000_000)), Some(1_800_000_000));
        assert_eq!(jwt_expiry("not-a-jwt"), None);
        assert_eq!(jwt_expiry("header.!!!not-base64!!!.sig"), None);
    }
}
