//! Getting a bearer token, cheapest source first.
//!
//! Foodstuffs' read APIs (search, specials, stores) need a bearer token but not
//! an account: loading the storefront sets an `fs-user-token` cookie holding a
//! short-lived JWT, and that JWT authorises the lot. An account token is a
//! different thing, minted through Club Plus (see [`crate::auth`]), and the two
//! are cached apart because they authorise different endpoints -- `/v1/edge/store`
//! answers a guest token with the store list and an account token with a flat
//! 400.

use std::path::{Path, PathBuf};
use std::time::Duration;

use net_kit::{wreq, Fault, Paths};
use serde::{Deserialize, Serialize};

use crate::banner::{Banner, Endpoints};
use crate::error::{Error, Result};

pub const COOKIE_NAME: &str = "fs-user-token";

/// Refresh rather than send a token this close to expiry. A token that expires
/// during the request it authorises fails in a way that reads as bad
/// credentials.
pub const SKEW: Duration = Duration::from_secs(60);

/// Tokens live about half an hour; assume less when the JWT will not parse.
const ASSUMED_LIFETIME: Duration = Duration::from_secs(25 * 60);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// Supplied by the caller.
    Override,
    Cache,
    /// A configured command that prints a token.
    Command,
    /// Minted from a stored Club Plus login.
    Login,
    /// Fetched from the storefront, anonymously.
    Storefront,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Source::Override => "supplied explicitly",
            Source::Cache => "reused from the local cache",
            Source::Command => "produced by the configured token_command",
            Source::Login => "minted from the stored Club Plus login",
            Source::Storefront => "minted from the storefront",
        }
    }

    /// Whether this token speaks for an account, or only for a visitor.
    pub fn is_account(self) -> bool {
        matches!(self, Source::Login)
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub token: String,
    pub expires_at_ms: u64,
    pub source: Source,
}

impl Token {
    pub fn expires_in(&self) -> Option<Duration> {
        let now = net_kit::jwt::now_ms();
        (self.expires_at_ms > now).then(|| Duration::from_millis(self.expires_at_ms - now))
    }

    pub fn fresh(&self) -> bool {
        net_kit::jwt::fresh(self.expires_at_ms, SKEW)
    }

    /// The `banner` claim: `NAT`, `MNW` or `PNS`.
    ///
    /// Worth reporting on its own, because a `NAT` token is not rejected by the
    /// cart -- it authenticates and answers with an empty cart belonging to
    /// nobody.
    pub fn banner_claim(&self) -> Option<String> {
        net_kit::jwt::claim_str(&self.token, "banner")
    }
}

/// Read the expiry a token declares, or assume one.
pub fn expiry_for(token: &str) -> u64 {
    net_kit::jwt::expiry_ms(token)
        .unwrap_or_else(|| net_kit::jwt::now_ms() + ASSUMED_LIFETIME.as_millis() as u64)
}

/// Where a banner's tokens are cached.
///
/// Guest and account tokens go in separate files. They authorise different
/// things, so serving one in place of the other produces a failure that looks
/// like a broken login.
pub fn cache_file(paths: &Paths, banner: Banner, guest: bool) -> PathBuf {
    let name = if guest {
        "guest-token.json"
    } else {
        "token.json"
    };
    paths.scoped(banner.id()).state_file(name)
}

#[derive(Serialize, Deserialize)]
struct Cached {
    token: String,
    expires_at_ms: u64,
}

pub fn read_cache(file: &Path) -> Option<Token> {
    let cached: Cached = net_kit::config::load_json_cache(file)?;
    Some(Token {
        token: cached.token,
        expires_at_ms: cached.expires_at_ms,
        source: Source::Cache,
    })
}

/// Best effort: a failed write costs the next command a mint, not this one its
/// result.
pub fn write_cache(file: &Path, token: &str, expires_at_ms: u64) {
    net_kit::config::save_json_cache(
        file,
        &Cached {
            token: token.to_string(),
            expires_at_ms,
        },
    );
}

/// The cached token for a banner without minting one, for reporting.
pub fn peek(paths: &Paths, banner: Banner, guest: bool) -> Option<Token> {
    read_cache(&cache_file(paths, banner, guest))
}

/// Load the storefront and take the `fs-user-token` cookie it sets.
///
/// No headers are set: the emulation already describes a browser loading a page,
/// and adding to it makes the request less like one, not more.
pub async fn mint_guest(
    http: &wreq::Client,
    banner: Banner,
    endpoints: &Endpoints,
) -> Result<String> {
    let url = format!("{}/", endpoints.origin);
    let sent = http.get(&url).send().await;
    let (headers, _body) = net_kit::http::text("GET", &url, sent).await?;

    net_kit::cookies::set_cookies(&headers)
        .remove(COOKIE_NAME)
        .filter(|v| !v.is_empty())
        .ok_or(Error::NoToken {
            banner,
            cookie: COOKIE_NAME,
            url,
            status: 200,
        })
}

/// Everything token resolution needs. A struct rather than ten arguments.
pub struct Request<'a> {
    pub http: &'a wreq::Client,
    pub banner: Banner,
    pub endpoints: &'a Endpoints,
    pub clubplus: &'a crate::banner::ClubPlusEndpoints,
    pub paths: &'a Paths,
    pub secrets: &'a net_kit::Secrets,
    /// A token supplied outright, which skips everything below.
    pub explicit: Option<&'a str>,
    /// A configured command that prints a token.
    pub token_command: Option<&'a str>,
    /// For renewing a session whose refresh token is gone.
    pub password: Option<&'a net_kit::password::Source>,
    /// The agent string the SSO exchange echoes back. Injected rather than read
    /// here, so it always matches the client that will send the request.
    pub user_agent: &'a str,
    /// Ask for an anonymous token even when logged in.
    ///
    /// Some endpoints are guest-scoped: `/v1/edge/store` answers a guest token
    /// with the store list and an account token with a flat 400.
    pub guest: bool,
    pub force_refresh: bool,
}

/// Resolve a usable token, cheapest source first.
///
/// explicit -> cache -> (guest ? storefront : token_command -> stored login ->
/// storefront). The storefront is the fallback for "no account yet" rather than
/// the normal path, because a guest token cannot read a cart.
pub async fn acquire(req: Request<'_>) -> Result<Token> {
    if let Some(token) = req.explicit.map(str::trim).filter(|t| !t.is_empty()) {
        return Ok(Token {
            expires_at_ms: expiry_for(token),
            token: token.to_string(),
            source: Source::Override,
        });
    }

    let file = cache_file(req.paths, req.banner, req.guest);
    if !req.force_refresh {
        if let Some(cached) = read_cache(&file) {
            if cached.fresh() {
                return Ok(cached);
            }
        }
    }

    if req.guest {
        let token = mint_guest(req.http, req.banner, req.endpoints).await?;
        let expires_at_ms = expiry_for(&token);
        write_cache(&file, &token, expires_at_ms);
        return Ok(Token {
            token,
            expires_at_ms,
            source: Source::Storefront,
        });
    }

    let (token, source) = match req.token_command.map(str::trim).filter(|c| !c.is_empty()) {
        Some(cmd) => (
            net_kit::run::capturing("token_command", cmd).await?,
            Source::Command,
        ),
        // A stored login is the normal path once someone has logged in; the
        // storefront is only reached for when nobody has.
        None => match crate::auth::session::load(req.secrets)?.is_some() {
            true => (account_token(&req).await?, Source::Login),
            false => (
                mint_guest(req.http, req.banner, req.endpoints).await?,
                Source::Storefront,
            ),
        },
    };

    let expires_at_ms = expiry_for(&token);
    write_cache(&file, &token, expires_at_ms);
    Ok(Token {
        token,
        expires_at_ms,
        source,
    })
}

/// Mint a banner token from the stored Club Plus login, renewing the session
/// first if it has aged out.
async fn account_token(req: &Request<'_>) -> Result<String> {
    let device_id = crate::auth::session::device_id(req.paths)?;
    let cfg = crate::auth::clubplus::Config {
        http: req.http,
        clubplus: req.clubplus,
        device_id: &device_id,
    };
    let active =
        crate::auth::session::active_session(&cfg, req.secrets, req.password, false).await?;

    match crate::auth::clubplus::banner_token(
        &cfg,
        req.banner,
        req.endpoints,
        &active.session,
        req.user_agent,
    )
    .await
    {
        Ok(token) => Ok(token),
        // A session whose `exp` still looked good can be rejected anyway: a
        // clock that disagrees with theirs, or a session ended elsewhere. One
        // forced renewal tells that apart from a login that is really gone --
        // and `renewed` is what stops it looping.
        Err(e) if !active.renewed && e.auth().is_some() => {
            let renewed =
                crate::auth::session::active_session(&cfg, req.secrets, req.password, true).await?;
            crate::auth::clubplus::banner_token(
                &cfg,
                req.banner,
                req.endpoints,
                &renewed.session,
                req.user_agent,
            )
            .await
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net_kit::jwt::now_ms;

    fn jwt(exp_secs: u64) -> String {
        use base64::Engine;
        let payload = serde_json::json!({ "exp": exp_secs, "banner": "MNW" }).to_string();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        format!("header.{encoded}.signature")
    }

    #[test]
    fn an_unparseable_token_still_gets_a_usable_expiry() {
        let expiry = expiry_for("not-a-jwt");
        assert!(expiry > now_ms(), "should be in the future");
        assert!(net_kit::jwt::fresh(expiry, SKEW));
    }

    #[test]
    fn an_expired_token_is_not_fresh() {
        let token = Token {
            token: jwt(1),
            expires_at_ms: expiry_for(&jwt(1)),
            source: Source::Cache,
        };
        assert!(!token.fresh());
        assert!(token.expires_in().is_none());
    }

    #[test]
    fn a_tokens_banner_claim_is_readable_without_verifying_it() {
        let raw = jwt(now_ms() / 1000 + 3600);
        let token = Token {
            expires_at_ms: expiry_for(&raw),
            token: raw,
            source: Source::Login,
        };
        assert_eq!(token.banner_claim().as_deref(), Some("MNW"));
        assert!(token.fresh());
    }

    #[test]
    fn guest_and_account_tokens_are_cached_in_separate_files() {
        // They authorise different endpoints; serving one for the other looks
        // like a broken login rather than a wrong cache.
        let paths = Paths::new("/cfg".into(), "/state".into());
        let guest = cache_file(&paths, Banner::NewWorld, true);
        let account = cache_file(&paths, Banner::NewWorld, false);
        assert_ne!(guest, account);
        assert!(guest.to_string_lossy().contains("newworld"));
        assert!(account.ends_with("token.json"));
    }

    #[test]
    fn the_two_banners_do_not_share_a_cache() {
        let paths = Paths::new("/cfg".into(), "/state".into());
        assert_ne!(
            cache_file(&paths, Banner::NewWorld, false),
            cache_file(&paths, Banner::PaknSave, false)
        );
    }

    #[test]
    fn a_cache_round_trips_and_a_corrupt_one_reads_as_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("newworld/token.json");
        assert!(read_cache(&file).is_none());
        write_cache(&file, "abc.def.ghi", 1_900_000_000_000);
        let back = read_cache(&file).unwrap();
        assert_eq!(back.token, "abc.def.ghi");
        assert_eq!(back.source, Source::Cache);

        std::fs::write(&file, "{ truncated").unwrap();
        assert!(read_cache(&file).is_none());
    }

    #[test]
    fn only_a_login_token_speaks_for_an_account() {
        assert!(Source::Login.is_account());
        for s in [
            Source::Storefront,
            Source::Cache,
            Source::Override,
            Source::Command,
        ] {
            assert!(!s.is_account(), "{s:?}");
        }
    }
}
