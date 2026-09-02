//! Cookies kept between runs.
//!
//! A cold start is scored as a new visitor. Only [`functional`] cookies are
//! stored, in the secret store beside the login. Session cookies are dropped at
//! exit; the rest carry the host's expiry and are discarded past it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use wreq::cookie::{CookieStore, IntoCookie};
use wreq::header::HeaderValue;

use crate::secrets::Secrets;

/// Where the kept cookies are filed in the secret store.
const ACCOUNT: &str = "cookies";

/// Cloudflare's cookies, and the storefront's credentials. Nothing else:
/// analytics, the store picker and UI state do not belong in a secret store.
fn functional(name: &str) -> bool {
    name.starts_with("__cf")
        || name.starts_with("_cf")
        || name == "cf_clearance"
        || matches!(name, "fs-user-token" | "refresh_token" | "API_TOKEN")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize)]
struct Kept {
    host: String,
    name: String,
    value: String,
    /// Unix seconds. Session cookies are not kept, so always set.
    expires_at: u64,
}

/// A jar that also records the functional cookies for persisting. Domain, path
/// and secure matching stay the inner [`wreq::cookie::Jar`]'s.
pub struct Jar {
    inner: wreq::cookie::Jar,
    kept: RwLock<BTreeMap<(String, String), Kept>>,
    dirty: AtomicBool,
}

impl Jar {
    /// Discards anything expired. An unreadable store is not an error; the run
    /// mints what it needs.
    pub fn load(secrets: &Secrets) -> Jar {
        let jar = Jar {
            inner: wreq::cookie::Jar::default(),
            kept: RwLock::new(BTreeMap::new()),
            dirty: AtomicBool::new(false),
        };
        let Ok(Some(raw)) = secrets.get(ACCOUNT) else {
            return jar;
        };
        let Ok(stored) = serde_json::from_str::<Vec<Kept>>(&raw) else {
            return jar;
        };

        let now = now();
        let mut kept = BTreeMap::new();
        for cookie in stored.into_iter().filter(|c| c.expires_at > now) {
            let Ok(uri) = format!("https://{}/", cookie.host).parse::<wreq::Uri>() else {
                continue;
            };
            jar.inner
                .add(format!("{}={}; Path=/", cookie.name, cookie.value), &uri);
            kept.insert((cookie.host.clone(), cookie.name.clone()), cookie);
        }
        if let Ok(mut guard) = jar.kept.write() {
            *guard = kept;
        }
        jar
    }

    /// Best-effort, like the token cache: a failed write costs the next run a
    /// mint, not this one.
    pub fn save(&self, secrets: &Secrets) {
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return;
        }
        let Ok(kept) = self.kept.read() else { return };
        let now = now();
        let live: Vec<&Kept> = kept.values().filter(|c| c.expires_at > now).collect();
        if let Ok(text) = serde_json::to_string(&live) {
            let _ = secrets.set(ACCOUNT, &text);
        }
    }

    pub fn clear(secrets: &Secrets) -> anyhow::Result<bool> {
        secrets.delete(ACCOUNT)
    }

    fn remember(&self, uri: &wreq::Uri, raw: &str) {
        let Some(host) = uri.host() else { return };
        let Some(cookie) = raw.into_cookie() else {
            return;
        };
        if !functional(cookie.name()) {
            return;
        }
        // No expiry: a session cookie. Dropped at exit, as a browser does.
        let expires_at = match (cookie.expires(), cookie.max_age()) {
            (Some(at), _) => at
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            (None, Some(age)) => now() + age.as_secs(),
            (None, None) => return,
        };
        let entry = Kept {
            host: host.to_string(),
            name: cookie.name().to_string(),
            value: cookie.value().to_string(),
            expires_at,
        };
        if let Ok(mut kept) = self.kept.write() {
            kept.insert((entry.host.clone(), entry.name.clone()), entry);
            self.dirty.store(true, Ordering::Relaxed);
        }
    }
}

impl CookieStore for Jar {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, uri: &wreq::Uri) {
        // Consumed once, and the inner jar needs it too.
        let values: Vec<&HeaderValue> = headers.collect();
        for value in &values {
            if let Ok(raw) = value.to_str() {
                self.remember(uri, raw);
            }
        }
        self.inner.set_cookies(&mut values.into_iter(), uri);
    }

    fn cookies(&self, uri: &wreq::Uri, version: wreq::Version) -> wreq::cookie::Cookies {
        self.inner.cookies(uri, version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_cloudflares_and_the_storefronts_credentials_only() {
        for name in [
            "__cf_bm",
            "_cfuvid",
            "cf_clearance",
            "fs-user-token",
            "refresh_token",
        ] {
            assert!(functional(name), "{name} should be kept");
        }
        // Analytics and UI state, which have no business in a secret store.
        for name in [
            "_dyid",
            "_dy_soct",
            "_dyjsession",
            "STORE_ID_V2",
            "Region",
            "orderDetailsTCs",
            "fs-store-select-tooltip-closed",
        ] {
            assert!(!functional(name), "{name} should not be kept");
        }
    }

    fn jar_with(raw: &str) -> Jar {
        let jar = Jar {
            inner: wreq::cookie::Jar::default(),
            kept: RwLock::new(BTreeMap::new()),
            dirty: AtomicBool::new(false),
        };
        let uri: wreq::Uri = "https://www.newworld.co.nz/".parse().unwrap();
        jar.remember(&uri, raw);
        jar
    }

    #[test]
    fn a_session_cookie_is_not_carried_past_this_process() {
        // No Max-Age and no Expires: a browser drops it when it closes.
        let jar = jar_with("__cf_bm=abc; Path=/; HttpOnly");
        assert!(jar.kept.read().unwrap().is_empty());
        assert!(!jar.dirty.load(Ordering::Relaxed));
    }

    #[test]
    fn an_expiring_cookie_is_kept_with_the_expiry_the_host_gave_it() {
        let jar = jar_with("__cf_bm=abc; Max-Age=1800; Path=/");
        let kept = jar.kept.read().unwrap();
        let entry = kept
            .get(&("www.newworld.co.nz".to_string(), "__cf_bm".to_string()))
            .expect("kept");
        assert_eq!(entry.value, "abc");
        assert!(entry.expires_at > now(), "should not already be expired");
        assert!(entry.expires_at <= now() + 1800);
    }

    #[test]
    fn an_unwanted_cookie_does_not_mark_the_jar_dirty() {
        let jar = jar_with("_dyid=tracking; Max-Age=99999; Path=/");
        assert!(jar.kept.read().unwrap().is_empty());
        assert!(!jar.dirty.load(Ordering::Relaxed));
    }
}
