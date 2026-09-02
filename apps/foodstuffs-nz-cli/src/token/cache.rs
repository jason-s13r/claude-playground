//! The on-disk token cache: one small JSON file per banner under the state
//! directory, so a token minted by one command is reused by the next.

use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;

use crate::banner::Banner;
use crate::config::{restrict, Paths};
use crate::token::{now_ms, GuestToken, Source};

/// Tokens live ~30 minutes; assume less when the JWT will not parse.
const ASSUMED_LIFETIME: Duration = Duration::from_secs(25 * 60);

#[derive(Serialize, Deserialize)]
pub(crate) struct CachedToken {
    pub token: String,
    pub expires_at_ms: u64,
}

pub(crate) fn expiry_for(token: &str) -> u64 {
    crate::auth::jwt::expiry_ms(token)
        .unwrap_or_else(|| now_ms() + ASSUMED_LIFETIME.as_millis() as u64)
}

pub(crate) fn read_cache(file: &std::path::Path) -> Option<CachedToken> {
    let text = fs::read_to_string(file).ok()?;
    serde_json::from_str(&text).ok()
}

/// A cache miss is never fatal -- worst case we fetch a token again.
pub(crate) fn write_cache(file: &std::path::Path, token: &str, expires_at_ms: u64) {
    if let Some(dir) = file.parent() {
        if fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let body = CachedToken {
        token: token.to_string(),
        expires_at_ms,
    };
    if let Ok(text) = serde_json::to_string(&body) {
        if fs::write(file, text).is_ok() {
            restrict(file);
        }
    }
}

/// Remember a token that has already been proved to work, so the next command
/// reuses it instead of minting another.
pub fn cache_account_token(paths: &Paths, banner: Banner, token: &str) {
    write_cache(&paths.token_file(banner), token, expiry_for(token));
}

/// The cached account token for a banner, if any, without minting one.
/// `fsnz auth status` reports what is on disk rather than causing traffic.
pub fn peek_cache(paths: &Paths, banner: Banner) -> Option<GuestToken> {
    let cached = read_cache(&paths.token_file(banner))?;
    Some(GuestToken {
        token: cached.token,
        expires_at_ms: cached.expires_at_ms,
        source: Source::Cache,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    use crate::token::fresh;

    fn jwt_with_exp(exp_secs: u64) -> String {
        let payload = serde_json::json!({ "exp": exp_secs }).to_string();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        format!("header.{encoded}.signature")
    }

    #[test]
    fn falls_back_to_an_assumed_lifetime_for_an_unparseable_token() {
        let expiry = expiry_for("not-a-jwt");
        assert!(expiry > now_ms(), "expiry should still be in the future");
        assert!(fresh(expiry));
    }

    #[test]
    fn an_expired_token_is_not_fresh() {
        assert!(!fresh(expiry_for(&jwt_with_exp(1))));
    }
}
