//! Logging in through Club Plus.
//!
//! Foodstuffs put their accounts behind Club Plus, and -- unlike the
//! storefronts -- none of it needs a browser. Three calls:
//!
//! 1. `login.clubplus.co.nz/api/apigee-credentials` hands out a bearer token to
//!    anyone who asks; it is the key for the login API itself.
//! 2. `POST .../user/login` exchanges an email and password for a Club Plus
//!    session, or -- from an unrecognised device -- for a code emailed to the
//!    account, redeemed at `POST .../user/tfa/login`.
//! 3. `POST {clubplus api}/user/token/secure` issues a single-use code scoped
//!    to one banner, and the storefront's `/api/user/login/sso` swaps that for
//!    the `fs-user-token` the banner's own API wants.
//!
//! **Step 3 has to go to Club Plus.** The banner API answers the same path --
//! 200, with a plausible `secure_token` -- but the code it issues ignores the
//! `banner` field and exchanges back into a national (`NAT`) token. Nothing
//! fails; the cart endpoints just quietly answer a NAT token with an empty cart
//! belonging to nobody. Only the Club Plus code exchanges into `MNW`/`PNS`.

use net_kit::wreq;
use serde::Deserialize;

use crate::banner::{Banner, ClubPlusEndpoints, Endpoints};
use crate::error::{Error, Result};

#[derive(Clone)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

/// Redacted by hand. These are bearer credentials, and a derived `Debug` puts
/// them into panic messages, `{:?}` logs and anything that formats an error.
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Everything the chain needs that is not a credential.
pub struct Config<'a> {
    pub http: &'a wreq::Client,
    pub clubplus: &'a ClubPlusEndpoints,
    /// A stable per-installation identifier. The login API rejects a request
    /// without one, and a new one on every login presents a new device.
    pub device_id: &'a str,
}

/// A session, or a demand for the emailed code.
#[derive(Debug)]
pub enum Login {
    Complete(Session),
    ChallengeRequired(Challenge),
}

/// A login held for an emailed code. These tokens authorise nothing else and
/// are never stored.
pub struct Challenge {
    /// How the code was sent, as Club Plus names it.
    pub method: String,
    pre_auth_token: String,
    phv_token: String,
}

impl std::fmt::Debug for Challenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Challenge")
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct ApigeeCredentials {
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct LoginResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    #[serde(rename = "isEmailVerified")]
    is_email_verified: Option<bool>,
    /// Set when the password alone was not enough and a one-time code has been
    /// sent. The tokens above are then only good for `/user/tfa/login`.
    #[serde(rename = "isTFARequired")]
    is_tfa_required: Option<bool>,
    /// How the code was sent. `EMAIL_OTP` is the only value seen.
    #[serde(rename = "tfaMethod")]
    tfa_method: Option<String>,
    /// Ties the code back to this login attempt.
    #[serde(rename = "phvToken")]
    phv_token: Option<String>,
}

/// A response with its body read: a challenge is only recognisable from the
/// body, so status and body are always needed together.
struct Reply {
    status: u16,
    body: String,
}

impl Reply {
    fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    fn as_error(&self, method: &'static str, url: &str) -> Error {
        Error::Http(net_kit::HttpError::Status {
            method,
            url: url.to_string(),
            status: self.status,
            detail: net_kit::error::truncate(&self.body, 300),
            body: self.body.clone(),
        })
    }
}

/// What a browser's `fetch()` adds over the emulation's defaults. No
/// `User-Agent`: the emulation owns it, and overriding it here would make the
/// header and the handshake disagree.
fn xhr(req: wreq::RequestBuilder, origin: &str) -> wreq::RequestBuilder {
    req.header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header("Origin", origin)
        .header("Referer", format!("{origin}/"))
}

async fn send(req: wreq::RequestBuilder, method: &'static str, url: &str) -> Result<Reply> {
    let res = req
        .send()
        .await
        .map_err(|source| net_kit::HttpError::Transport {
            method,
            url: url.to_string(),
            source,
        })?;
    Ok(Reply {
        status: res.status().as_u16(),
        body: res.text().await.unwrap_or_default(),
    })
}

/// The public bearer token that authorises the login API. No account involved --
/// the login page fetches this before anyone has typed anything.
pub async fn apigee_token(cfg: &Config<'_>) -> Result<String> {
    let url = format!("{}/api/apigee-credentials", cfg.clubplus.login);
    let res = send(xhr(cfg.http.get(&url), &cfg.clubplus.login), "GET", &url).await?;
    challenged(&cfg.clubplus.login, &res)?;
    if !res.is_success() {
        return Err(res.as_error("GET", &url));
    }
    serde_json::from_str::<ApigeeCredentials>(&res.body)
        .ok()
        .and_then(|c| c.access_token)
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| {
            Error::Shape(format!(
                "{url} returned no access_token: {}",
                fields(&res.body)
            ))
        })
}

/// Exchange an email and password for a session, or for a verification
/// challenge.
pub async fn login(cfg: &Config<'_>, email: &str, password: &str) -> Result<Login> {
    let apigee = apigee_token(cfg).await?;
    let url = format!("{}/user/login", cfg.clubplus.api);
    let body = serde_json::json!({ "email": email, "password": password, "source": "WEB" });
    let res = send(
        xhr(cfg.http.post(&url), &cfg.clubplus.login)
            .header("Authorization", format!("Bearer {apigee}"))
            // Without this the API answers "Missing required header: x-device-id".
            .header("x-device-id", cfg.device_id)
            .body(body.to_string()),
        "POST",
        &url,
    )
    .await?;
    challenged(&cfg.clubplus.login, &res)?;
    if !res.is_success() {
        return Err(res.as_error("POST", &url));
    }

    let parsed = parse_login(&res.body)?;
    let access_token = parsed
        .access_token
        .clone()
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| Error::Shape("Club Plus login returned no access_token".into()))?;

    // The code is already sent; these tokens do nothing until it comes back.
    if parsed.is_tfa_required == Some(true) {
        let phv_token = parsed
            .phv_token
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| {
                Error::Shape("Club Plus asked for a code but sent no phvToken".into())
            })?;
        return Ok(Login::ChallengeRequired(Challenge {
            method: parsed.tfa_method.unwrap_or_else(|| "EMAIL_OTP".into()),
            pre_auth_token: access_token,
            phv_token,
        }));
    }
    verified(&parsed)?;
    Ok(Login::Complete(Session {
        access_token,
        refresh_token: parsed.refresh_token.filter(|t| !t.trim().is_empty()),
    }))
}

/// Redeem the emailed code.
///
/// Authorised by the pre-TFA token, not the apigee one, and carries no
/// `x-device-id`: `phvToken` already pins the device.
pub async fn complete_challenge(
    cfg: &Config<'_>,
    challenge: &Challenge,
    code: &str,
) -> Result<Session> {
    let url = format!("{}/user/tfa/login", cfg.clubplus.api);
    let body = serde_json::json!({ "code": code.trim(), "phvToken": challenge.phv_token });
    let res = send(
        xhr(cfg.http.post(&url), &cfg.clubplus.login)
            .header(
                "Authorization",
                format!("Bearer {}", challenge.pre_auth_token),
            )
            .body(body.to_string()),
        "POST",
        &url,
    )
    .await?;
    challenged(&cfg.clubplus.login, &res)?;
    if !res.is_success() {
        return Err(res.as_error("POST", &url));
    }

    let parsed = parse_login(&res.body)?;
    verified(&parsed)?;
    Ok(Session {
        access_token: parsed
            .access_token
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| {
                Error::Shape("Club Plus accepted the code but returned no token".into())
            })?,
        refresh_token: parsed.refresh_token.filter(|t| !t.trim().is_empty()),
    })
}

/// Swap the refresh token for a new session.
///
/// The Club Plus access token lasts about half an hour, the same as the banner
/// tokens minted from it, so without this a login is only good for one sitting.
///
/// The refresh token is **rotated**: the response carries a replacement and the
/// one just sent stops working. Dropping that replacement means logging in
/// again, so a caller persists it before doing anything else.
pub async fn refresh(cfg: &Config<'_>, refresh_token: &str) -> Result<Session> {
    let apigee = apigee_token(cfg).await?;
    let url = format!("{}/user/login/refresh", cfg.clubplus.api);
    // `refreshToken` and nothing else. The storefront's own server-side code
    // sends `banner` and `sourceApplication` too, but this endpoint rejects
    // both with "excess property and therefore is not allowed".
    let body = serde_json::json!({ "refreshToken": refresh_token });
    let res = send(
        xhr(cfg.http.post(&url), &cfg.clubplus.login)
            .header("Authorization", format!("Bearer {apigee}"))
            .header("x-device-id", cfg.device_id)
            .body(body.to_string()),
        "POST",
        &url,
    )
    .await?;
    challenged(&cfg.clubplus.login, &res)?;
    if res.status == 401 || res.status == 400 {
        return Err(Error::RefreshRejected);
    }
    if !res.is_success() {
        return Err(res.as_error("POST", &url));
    }

    let parsed = parse_login(&res.body)?;
    Ok(Session {
        access_token: parsed
            .access_token
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| Error::Shape("Club Plus renewed but returned no token".into()))?,
        // A response without one leaves the old token in play rather than
        // dropping the ability to renew again.
        refresh_token: parsed
            .refresh_token
            .filter(|t| !t.trim().is_empty())
            .or_else(|| Some(refresh_token.to_string())),
    })
}

/// Turn a Club Plus session into the banner's own `fs-user-token`.
///
/// Two steps, because the first returns an exchange code rather than a token.
pub async fn banner_token(
    cfg: &Config<'_>,
    banner: Banner,
    endpoints: &Endpoints,
    session: &Session,
    user_agent: &str,
) -> Result<String> {
    let secure = secure_token(cfg, banner, session).await?;
    exchange_secure_token(cfg, banner, endpoints, &secure, user_agent).await
}

/// Step one: a single-use code tying the Club Plus session to one banner.
///
/// This goes to **Club Plus**, never to the banner API. See the module header:
/// the banner API answers 200 with a token that silently scopes back to NAT.
async fn secure_token(cfg: &Config<'_>, banner: Banner, session: &Session) -> Result<String> {
    let url = format!("{}/user/token/secure", cfg.clubplus.api);
    let body = serde_json::json!({ "banner": banner.code(), "source": "WEB" });
    let res = send(
        xhr(cfg.http.post(&url), &cfg.clubplus.login)
            .header("Authorization", format!("Bearer {}", session.access_token))
            .header("x-device-id", cfg.device_id)
            .body(body.to_string()),
        "POST",
        &url,
    )
    .await?;
    challenged(&cfg.clubplus.login, &res)?;

    // Not necessarily a stale session: a held device reads as a 401 too, and
    // logging in again will not clear that.
    if res.status == 401 {
        return Err(Error::Unauthorised { banner });
    }
    if !res.is_success() {
        return Err(res.as_error("POST", &url));
    }
    serde_json::from_str::<serde_json::Value>(&res.body)
        .ok()
        .and_then(|v| {
            v.get("secure_token")
                .or_else(|| v.get("secureToken"))
                .and_then(|t| t.as_str())
                .map(str::to_string)
        })
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| {
            Error::Shape(format!(
                "{banner} issued no secure_token; the response carried: {}",
                fields(&res.body)
            ))
        })
}

/// Step two: swap the code for the token, at the storefront rather than the API.
async fn exchange_secure_token(
    cfg: &Config<'_>,
    banner: Banner,
    endpoints: &Endpoints,
    secure: &str,
    user_agent: &str,
) -> Result<String> {
    let url = format!("{}/api/user/login/sso", endpoints.origin);
    let body = serde_json::json!({
        "key": secure,
        "forceNewSession": false,
        // Echoed in the body rather than sent as a header, so this is the one
        // place the agent string is named -- and it has to be the one the
        // handshake implies.
        "fingerprintGuest": user_agent,
    });
    let res = send(
        xhr(cfg.http.post(&url), &endpoints.origin).body(body.to_string()),
        "POST",
        &url,
    )
    .await?;
    challenged(&endpoints.origin, &res)?;
    if !res.is_success() {
        return Err(res.as_error("POST", &url));
    }
    token_from(&res.body, banner)
}

/// Pull the banner token out of a response whose exact shape is unconfirmed.
///
/// Rather than guess one field name and fail opaquely, try the plausible ones
/// and -- on a miss -- report the field *names* that were present, which is the
/// one piece of information needed to fix this. Names only: the values are
/// credentials.
fn token_from(body: &str, banner: Banner) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| Error::decode(format!("{banner} did not answer with JSON"), e))?;

    for key in [
        "access_token",
        "accessToken",
        "token",
        "fs-user-token",
        "fsUserToken",
        "key",
    ] {
        if let Some(t) = value.get(key).and_then(|v| v.as_str()) {
            if !t.trim().is_empty() {
                return Ok(t.to_string());
            }
        }
    }
    Err(Error::Shape(format!(
        "{banner} returned no recognised token field. The response carried: {}",
        fields(body)
    )))
}

fn parse_login(body: &str) -> Result<LoginResponse> {
    serde_json::from_str(body).map_err(|e| Error::decode("parsing the Club Plus response", e))
}

fn verified(parsed: &LoginResponse) -> Result<()> {
    if parsed.is_email_verified == Some(false) {
        return Err(Error::Shape(
            "that Club Plus account's email address is not verified".into(),
        ));
    }
    Ok(())
}

/// The field names a response carried. Names only.
fn fields(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>().join(", "))
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "no object fields".to_string())
}

/// Cloudflare's challenge answers with an HTML interstitial in place of the
/// API's error, so quoting the body would paste a page of markup.
pub fn is_challenge(body: &str) -> bool {
    let head = body.chars().take(2000).collect::<String>().to_lowercase();
    // Both APIs answer in JSON; a page of HTML is already the tell.
    if !head.trim_start().starts_with('<') {
        return false;
    }
    head.contains("just a moment")
        || head.contains("cf-browser-verification")
        || head.contains("challenge-platform")
        || head.contains("cf-chl-")
}

/// A challenge is its own error, carrying no status code: a renewal cannot
/// clear a bot check, and reporting it as an auth failure would spend one.
fn challenged(host: &str, res: &Reply) -> Result<()> {
    if is_challenge(&res.body) {
        return Err(Error::Challenged {
            host: host.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_token_under_any_of_the_likely_names() {
        for key in [
            "access_token",
            "accessToken",
            "token",
            "fs-user-token",
            "key",
        ] {
            let body = serde_json::json!({ key: "abc.def.ghi" }).to_string();
            assert_eq!(
                token_from(&body, Banner::NewWorld).unwrap(),
                "abc.def.ghi",
                "key {key}"
            );
        }
    }

    #[test]
    fn an_unknown_shape_reports_the_field_names_it_did_see() {
        let body = serde_json::json!({ "sessionThing": "x", "ttl": 60 }).to_string();
        let err = token_from(&body, Banner::NewWorld).unwrap_err().to_string();
        assert!(err.contains("sessionThing"), "{err}");
        assert!(err.contains("ttl"), "{err}");
        assert!(!err.contains('x'), "values are credentials: {err}");
    }

    #[test]
    fn an_empty_token_does_not_count_as_found() {
        let body = serde_json::json!({ "access_token": "   ", "other": 1 }).to_string();
        assert!(token_from(&body, Banner::NewWorld).is_err());
    }

    #[test]
    fn a_bot_check_is_told_apart_from_an_api_error() {
        assert!(is_challenge(
            "<!DOCTYPE html><html lang=\"en-US\"><head><title>Just a moment...</title>"
        ));
        // A JSON body merely containing the words is not a challenge.
        assert!(!is_challenge(
            &serde_json::json!({ "message": "just a moment ago the token expired" }).to_string()
        ));
    }

    #[test]
    fn field_names_survive_a_body_that_is_not_an_object() {
        assert_eq!(fields("[1,2]"), "no object fields");
        assert_eq!(fields("not json"), "no object fields");
    }
}
