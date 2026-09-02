//! Guest tokens.
//!
//! Foodstuffs' read APIs (search, specials, stores) need a bearer token but not
//! an account: loading the storefront sets an `fs-user-token` cookie holding a
//! short-lived JWT, and that JWT authorises the lot. So there is no login here
//! -- just a token to fetch, cache until it expires, and send.

pub mod cache;

pub use cache::{cache_account_token, peek_cache};

use anyhow::{bail, Context, Result};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::auth;
use crate::banner::{Banner, Endpoints};
use crate::config::{BannerConfig, Paths};
use crate::secrets::Secrets;
use crate::token::cache::{expiry_for, read_cache, write_cache};

const COOKIE_NAME: &str = "fs-user-token";
/// Refresh rather than send a token this close to expiry.
const SKEW: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// `--token` or `FSNZ_TOKEN`.
    Override,
    Cache,
    /// The `token_command` configured for this banner.
    Command,
    /// Minted from a stored Club Plus login.
    Login,
    /// Fetched from the storefront.
    Storefront,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Source::Override => "supplied via --token/FSNZ_TOKEN",
            Source::Cache => "reused from the local cache",
            Source::Command => "produced by the configured token_command",
            Source::Login => "minted from the stored Club Plus login",
            Source::Storefront => "minted from the storefront",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GuestToken {
    pub token: String,
    pub expires_at_ms: u64,
    pub source: Source,
}

impl GuestToken {
    pub fn expires_in(&self) -> Option<Duration> {
        let now = now_ms();
        (self.expires_at_ms > now).then(|| Duration::from_millis(self.expires_at_ms - now))
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn fresh(expires_at_ms: u64) -> bool {
    expires_at_ms.saturating_sub(now_ms()) > SKEW.as_millis() as u64
}

/// Everything token resolution needs. A struct rather than eight arguments.
pub struct Request<'a> {
    pub http: &'a wreq::Client,
    pub banner: Banner,
    pub endpoints: &'a Endpoints,
    pub paths: &'a Paths,
    pub cfg: &'a BannerConfig,
    pub secrets: &'a Secrets,
    /// `--token` or a per-banner environment variable.
    pub explicit: Option<&'a str>,
    /// Ask for an anonymous token even when logged in.
    ///
    /// Some endpoints are guest-scoped: `/v1/edge/store` answers a guest token
    /// with the store list and an account token with a flat 400. Store listings
    /// are public, so those callers ask for a guest token explicitly.
    pub guest: bool,
    pub force_refresh: bool,
}

/// Resolve a usable token, cheapest source first.
pub async fn acquire(req: Request<'_>) -> Result<GuestToken> {
    if let Some(token) = req.explicit.map(str::trim).filter(|t| !t.is_empty()) {
        return Ok(GuestToken {
            expires_at_ms: expiry_for(token),
            token: token.to_string(),
            source: Source::Override,
        });
    }

    let cache_file = if req.guest {
        req.paths.guest_token_file(req.banner)
    } else {
        req.paths.token_file(req.banner)
    };

    if !req.force_refresh {
        if let Some(cached) = read_cache(&cache_file) {
            if fresh(cached.expires_at_ms) {
                return Ok(GuestToken {
                    token: cached.token,
                    expires_at_ms: cached.expires_at_ms,
                    source: Source::Cache,
                });
            }
        }
    }

    if req.guest {
        let token = mint(req.http, req.banner, req.endpoints).await?;
        let expires_at_ms = expiry_for(&token);
        write_cache(&cache_file, &token, expires_at_ms);
        return Ok(GuestToken {
            token,
            expires_at_ms,
            source: Source::Storefront,
        });
    }

    let (token, source) = match req.cfg.token_command.as_deref() {
        Some(cmd) if !cmd.trim().is_empty() => {
            (crate::process::run::capturing(cmd).await?, Source::Command)
        }
        // A stored login is the normal path once `fsnz auth login` has been run; the
        // storefront is only reached for when it has not.
        _ => match auth::load(req.secrets)?.is_some() {
            true => (account_token(&req).await?, Source::Login),
            false => (
                mint(req.http, req.banner, req.endpoints).await?,
                Source::Storefront,
            ),
        },
    };

    let expires_at_ms = expiry_for(&token);
    write_cache(&cache_file, &token, expires_at_ms);
    Ok(GuestToken {
        token,
        expires_at_ms,
        source,
    })
}

/// Mint a banner token from the stored Club Plus login, renewing the session
/// first if it has aged out.
///
/// The Club Plus access token expires on the same half-hour clock as the banner
/// tokens it mints, so without the renewal in `active_session` a login would be
/// good for one sitting and no longer.
async fn account_token(req: &Request<'_>) -> Result<String> {
    let device_id = auth::device_id(req.paths)?;
    let active = auth::active_session(req.http, req.secrets, req.paths, false).await?;

    match auth::banner_token(
        req.http,
        req.banner,
        req.endpoints,
        &active.session,
        &device_id,
    )
    .await
    {
        Ok(token) => Ok(token),
        // A session whose `exp` still looked good can be rejected anyway: a
        // clock that disagrees with theirs, or a session ended elsewhere. One
        // forced renewal tells that apart from a login that is really gone.
        Err(e) if !active.renewed && is_unauthorised(&e) => {
            let renewed = auth::active_session(req.http, req.secrets, req.paths, true).await?;
            auth::banner_token(
                req.http,
                req.banner,
                req.endpoints,
                &renewed.session,
                &device_id,
            )
            .await
        }
        Err(e) => Err(e),
    }
}

fn is_unauthorised(e: &anyhow::Error) -> bool {
    let text = format!("{e:#}");
    text.contains("401") || text.contains("403")
}

/// Load the storefront and take the `fs-user-token` cookie it sets. No headers
/// are set: the emulation already describes a browser loading a page.
async fn mint(http: &wreq::Client, banner: Banner, endpoints: &Endpoints) -> Result<String> {
    let url = format!("{}/", endpoints.origin);
    let res = http
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = res.status();

    let token = res
        .headers()
        .get_all(wreq::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(cookie_value);
    if let Some(token) = token {
        return Ok(token);
    }

    bail!(
        "{} returned no {COOKIE_NAME} cookie from {url} (HTTP {status}).\n\
         Run `fsnz auth login`, or export FSNZ_TOKEN=<value> copied from a browser \
         (DevTools -> Application -> Cookies -> {COOKIE_NAME}).",
        banner.name(),
    )
}

/// Pull `fs-user-token=<value>` out of one Set-Cookie header line.
fn cookie_value(header: &str) -> Option<String> {
    let prefix = format!("{COOKIE_NAME}=");
    let start = header.find(&prefix)? + prefix.len();
    let rest = &header[start..];
    let value = rest.split(';').next().unwrap_or(rest).trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_cookie_from_a_set_cookie_header() {
        let header = "fs-user-token=abc.def.ghi; Path=/; HttpOnly; Secure; SameSite=Lax";
        assert_eq!(cookie_value(header).as_deref(), Some("abc.def.ghi"));
        assert_eq!(cookie_value("other=1; Path=/"), None);
    }
}
