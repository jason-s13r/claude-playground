//! Cookies kept between runs.
//!
//! A cold start is scored as a new visitor, so the bot-manager cookie a
//! previous run earned is worth keeping. Only cookies the caller's `keep`
//! predicate accepts are stored -- analytics and UI state have no business in a
//! credential store -- and they are stored *in* the credential store, because
//! the ones worth keeping are credentials. Session cookies (no expiry) are
//! dropped at exit, as a browser does.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use wreq::cookie::{CookieStore, IntoCookie};
use wreq::header::HeaderValue;

use crate::error::Result;
use crate::jwt::now_secs;
use crate::secrets::Secrets;

/// Which cookies survive a run. Named as a function pointer rather than a
/// closure so a `Jar` stays `Send + Sync` without a boxed trait object.
pub type Keep = fn(&str) -> bool;

#[derive(Clone, Serialize, Deserialize)]
struct Kept {
    host: String,
    name: String,
    value: String,
    /// Unix seconds. Session cookies are not kept, so this is always set.
    expires_at: u64,
}

/// A jar that also records the keepable cookies for persisting. Domain, path
/// and secure matching stay the inner [`wreq::cookie::Jar`]'s.
pub struct Jar {
    inner: wreq::cookie::Jar,
    kept: RwLock<BTreeMap<(String, String), Kept>>,
    dirty: AtomicBool,
    keep: Keep,
    account: String,
}

impl Jar {
    pub fn empty(account: impl Into<String>, keep: Keep) -> Jar {
        Jar {
            inner: wreq::cookie::Jar::default(),
            kept: RwLock::new(BTreeMap::new()),
            dirty: AtomicBool::new(false),
            keep,
            account: account.into(),
        }
    }

    /// Load what is stored, discarding anything expired.
    ///
    /// An unreadable store is not an error: the run mints what it needs.
    pub fn load(secrets: &Secrets, account: impl Into<String>, keep: Keep) -> Jar {
        let jar = Jar::empty(account, keep);
        let Ok(Some(raw)) = secrets.get(&jar.account) else {
            return jar;
        };
        let Ok(stored) = serde_json::from_str::<Vec<Kept>>(&raw) else {
            return jar;
        };

        let now = now_secs();
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

    /// Best effort, like a cache: a failed write costs the next run a fresh
    /// handshake, not this one its result.
    pub fn save(&self, secrets: &Secrets) {
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return;
        }
        let Ok(kept) = self.kept.read() else { return };
        let now = now_secs();
        let live: Vec<&Kept> = kept.values().filter(|c| c.expires_at > now).collect();
        if let Ok(text) = serde_json::to_string(&live) {
            let _ = secrets.set(&self.account, &text);
        }
    }

    pub fn clear(secrets: &Secrets, account: &str) -> Result<bool> {
        secrets.delete(account)
    }

    /// The current value of a cookie by name, whichever host set it.
    ///
    /// This is how a token that arrives as a `Set-Cookie` gets read back out.
    pub fn get(&self, name: &str) -> Option<String> {
        let kept = self.kept.read().ok()?;
        kept.values()
            .find(|c| c.name == name)
            .map(|c| c.value.clone())
    }

    /// Seed a cookie directly, for an imported session.
    pub fn insert(&self, host: &str, name: &str, value: &str, expires_at: u64) {
        let Ok(uri) = format!("https://{host}/").parse::<wreq::Uri>() else {
            return;
        };
        self.inner.add(format!("{name}={value}; Path=/"), &uri);
        if (self.keep)(name) {
            if let Ok(mut kept) = self.kept.write() {
                kept.insert(
                    (host.to_string(), name.to_string()),
                    Kept {
                        host: host.to_string(),
                        name: name.to_string(),
                        value: value.to_string(),
                        expires_at,
                    },
                );
                self.dirty.store(true, Ordering::Relaxed);
            }
        }
    }

    fn remember(&self, uri: &wreq::Uri, raw: &str) {
        let Some(host) = uri.host() else { return };
        let Some(cookie) = raw.into_cookie() else {
            return;
        };
        if !(self.keep)(cookie.name()) {
            return;
        }
        // No expiry at all: a session cookie, dropped at exit as a browser does.
        let expires_at = match (cookie.expires(), cookie.max_age()) {
            (Some(at), _) => at
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            (None, Some(age)) => now_secs() + age.as_secs(),
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

/// Every `Set-Cookie` on a response, as name/value pairs.
///
/// For a caller reading a token straight off a response rather than through a
/// jar -- which is how a guest token is minted.
pub fn set_cookies(headers: &wreq::header::HeaderMap) -> BTreeMap<String, String> {
    headers
        .get_all(wreq::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|raw| raw.split(';').next())
        .filter_map(|pair| pair.trim().split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// A `Cookie:` request header, as name/value pairs.
pub fn parse_cookie_header(header: &str) -> BTreeMap<String, String> {
    header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// Pull cookies out of a Netscape-format `cookies.txt`, which is what browser
/// export extensions and `curl -c` write.
///
/// Lines are tab-separated: domain, subdomains-flag, path, secure, expiry,
/// name, value. `domain_suffix` filters to one site, so a whole-browser export
/// can be handed over as-is.
pub fn from_netscape(text: &str, domain_suffix: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim_end)
        // `#HttpOnly_` is a real prefix on the domain field, not a comment.
        .map(|line| line.strip_prefix("#HttpOnly_").unwrap_or(line))
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 7 {
                return None;
            }
            let domain = fields[0].trim_start_matches('.');
            domain
                .ends_with(domain_suffix)
                .then(|| (fields[5].to_string(), fields[6].to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::Backend;

    /// A caller's policy: the bot-manager cookies and the credential, nothing
    /// else.
    fn keep(name: &str) -> bool {
        name.starts_with("__cf") || matches!(name, "cf_clearance" | "session-token")
    }

    fn jar_with(raw: &str) -> Jar {
        let jar = Jar::empty("cookies", keep);
        let uri: wreq::Uri = "https://www.example.test/".parse().unwrap();
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
    fn an_expiring_cookie_keeps_the_expiry_the_host_gave_it() {
        let jar = jar_with("__cf_bm=abc; Max-Age=1800; Path=/");
        assert_eq!(jar.get("__cf_bm").as_deref(), Some("abc"));
        let kept = jar.kept.read().unwrap();
        let entry = kept.values().next().unwrap();
        assert!(entry.expires_at > now_secs());
        assert!(entry.expires_at <= now_secs() + 1800);
    }

    #[test]
    fn a_cookie_the_policy_rejects_does_not_mark_the_jar_dirty() {
        let jar = jar_with("_analytics=tracking; Max-Age=99999; Path=/");
        assert!(jar.kept.read().unwrap().is_empty());
        assert!(!jar.dirty.load(Ordering::Relaxed));
    }

    #[test]
    fn survives_a_save_and_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let secrets = Secrets::new("net-kit-test", Backend::File, dir.path());
        let jar = jar_with("session-token=abc123; Max-Age=3600; Path=/");
        jar.save(&secrets);

        let reloaded = Jar::load(&secrets, "cookies", keep);
        assert_eq!(reloaded.get("session-token").as_deref(), Some("abc123"));
    }

    #[test]
    fn an_expired_cookie_is_dropped_on_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let secrets = Secrets::new("net-kit-test", Backend::File, dir.path());
        let jar = Jar::empty("cookies", keep);
        jar.insert("www.example.test", "session-token", "stale", now_secs() - 1);
        jar.save(&secrets);

        assert_eq!(
            Jar::load(&secrets, "cookies", keep).get("session-token"),
            None
        );
    }

    #[test]
    fn parses_a_request_cookie_header() {
        let got = parse_cookie_header("a=1; b=2 ;  c=3");
        assert_eq!(got.get("a").map(String::as_str), Some("1"));
        assert_eq!(got.get("c").map(String::as_str), Some("3"));
        assert_eq!(got.len(), 3);
    }

    #[test]
    fn reads_a_netscape_export_and_ignores_other_sites() {
        let text = "\
# Netscape HTTP Cookie File
.example.test\tTRUE\t/\tTRUE\t1900000000\tsession-token\tabc123
#HttpOnly_.example.test\tTRUE\t/\tTRUE\t1900000000\t__cf_bm\tcfvalue
.other-site.test\tTRUE\t/\tTRUE\t1900000000\tsession-token\tnot-ours
malformed line with too few fields
";
        let got = from_netscape(text, "example.test");
        assert_eq!(got.get("session-token").map(String::as_str), Some("abc123"));
        assert_eq!(
            got.get("__cf_bm").map(String::as_str),
            Some("cfvalue"),
            "#HttpOnly_ is a prefix, not a comment"
        );
        assert_eq!(got.len(), 2, "another site's cookies are not imported");
    }
}
