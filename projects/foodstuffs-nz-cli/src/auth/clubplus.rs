//! Logging in through Club Plus.
//!
//! Foodstuffs put their accounts behind Club Plus, and -- unlike the
//! storefronts -- none of it needs a browser. Three calls:
//!
//! 1. `login.clubplus.co.nz/api/apigee-credentials` hands out a bearer token to
//!    anyone who asks; it is the key for the login API itself.
//! 2. `POST .../user/login` exchanges an email and password for a Club Plus
//!    session.
//! 3. `POST {clubplus api}/user/token/secure` issues a single-use code scoped
//!    to one banner, and the storefront's `/api/user/login/sso` swaps that for
//!    the `fs-user-token` the banner's own API wants.
//!
//! Step 3 has to go to Club Plus. The banner API answers the same path -- 200,
//! with a plausible `secure_token` -- but the code it issues ignores the
//! `banner` field and exchanges back into a national (`NAT`) token. Nothing
//! fails; the cart endpoints just quietly answer a NAT token with an empty
//! cart belonging to nobody. Only the Club Plus code exchanges into `MNW`/`PNS`.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::env;

use crate::banner::{Banner, Endpoints};
use crate::process::curl as fetch;
use crate::token::USER_AGENT;

fn credentials_url() -> String {
    env::var("FSNZ_CLUBPLUS_LOGIN")
        .unwrap_or_else(|_| "https://login.clubplus.co.nz".into())
        .trim_end_matches('/')
        .to_string()
        + "/api/apigee-credentials"
}

fn clubplus_api() -> String {
    env::var("FSNZ_CLUBPLUS_API")
        .unwrap_or_else(|_| "https://api-prod.clubplus.co.nz/retail-fsl-online-edge".into())
        .trim_end_matches('/')
        .to_string()
}

#[derive(Clone, Debug)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: Option<String>,
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
}

const CLUBPLUS_ORIGIN: &str = "https://login.clubplus.co.nz";

fn base_headers(origin: &str) -> Vec<(&'static str, String)> {
    vec![
        ("User-Agent", USER_AGENT.to_string()),
        ("Accept", "application/json, text/plain, */*".to_string()),
        ("Content-Type", "application/json".to_string()),
        ("Origin", origin.to_string()),
        ("Referer", format!("{origin}/")),
    ]
}

/// The public bearer token that authorises the login API. No account involved --
/// the login page fetches this before anyone has typed anything.
pub async fn apigee_token() -> Result<String> {
    let url = credentials_url();
    let res = fetch::request("GET", &url, &base_headers(CLUBPLUS_ORIGIN), None).await?;
    if !res.is_success() {
        bail!(
            "Club Plus returned {} for {url}: {}",
            res.status,
            clip(&res.body, 200)
        );
    }
    serde_json::from_str::<ApigeeCredentials>(&res.body)
        .ok()
        .and_then(|c| c.access_token)
        .filter(|t| !t.trim().is_empty())
        .with_context(|| format!("{url} returned no access_token: {}", clip(&res.body, 200)))
}

/// Exchange an email and password for a Club Plus session.
pub async fn login(email: &str, password: &str, device_id: &str) -> Result<Session> {
    let apigee = apigee_token().await?;
    let url = format!("{}/user/login", clubplus_api());
    let mut headers = base_headers(CLUBPLUS_ORIGIN);
    headers.push(("Authorization", format!("Bearer {apigee}")));
    // Without this the API answers "Missing required header: x-device-id".
    headers.push(("x-device-id", device_id.to_string()));

    let body = serde_json::json!({ "email": email, "password": password, "source": "WEB" });
    let res = fetch::request("POST", &url, &headers, Some(&body.to_string())).await?;

    if res.status == 401 || res.status == 400 {
        bail!(
            "Club Plus rejected that email and password ({}).",
            res.status
        );
    }
    if !res.is_success() {
        bail!(
            "Club Plus login failed with {}: {}",
            res.status,
            clip(&res.body, 200)
        );
    }

    let parsed: LoginResponse = serde_json::from_str(&res.body).with_context(|| {
        format!(
            "parsing the Club Plus login response: {}",
            clip(&res.body, 200)
        )
    })?;
    let access_token = parsed
        .access_token
        .filter(|t| !t.trim().is_empty())
        .context("Club Plus login succeeded but returned no access_token")?;
    if parsed.is_email_verified == Some(false) {
        bail!("that Club Plus account's email address is not verified");
    }
    Ok(Session {
        access_token,
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
/// again, so callers persist it before doing anything else -- see
/// [`refreshed_session`].
pub async fn refresh(refresh_token: &str, device_id: &str) -> Result<Session> {
    let apigee = apigee_token().await?;
    let url = format!("{}/user/login/refresh", clubplus_api());
    let mut headers = base_headers(CLUBPLUS_ORIGIN);
    headers.push(("Authorization", format!("Bearer {apigee}")));
    headers.push(("x-device-id", device_id.to_string()));

    // `refreshToken` and nothing else. The storefront's own server-side code
    // sends `banner` and `sourceApplication` too, but this endpoint rejects
    // both with "excess property and therefore is not allowed".
    let body = serde_json::json!({ "refreshToken": refresh_token });
    let res = fetch::request("POST", &url, &headers, Some(&body.to_string())).await?;

    if res.status == 401 || res.status == 400 {
        bail!(
            "Club Plus would not renew the session ({}); run `fsnz auth login`",
            res.status
        );
    }
    if !res.is_success() {
        bail!(
            "Club Plus refresh failed with {}: {}",
            res.status,
            clip(&res.body, 200)
        );
    }

    let parsed: LoginResponse = serde_json::from_str(&res.body)
        .with_context(|| format!("parsing the refresh response: {}", clip(&res.body, 200)))?;
    let access_token = parsed
        .access_token
        .filter(|t| !t.trim().is_empty())
        .context("Club Plus renewed the session but returned no access_token")?;
    Ok(Session {
        access_token,
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
/// Two steps, because the first returns an exchange code rather than a token:
/// `/user/token/secure` issues a single-use `secure_token` (a UUID), and the
/// banner's own `/api/user/login/sso` swaps that for the JWT the API wants.
pub async fn banner_token(
    banner: Banner,
    endpoints: &Endpoints,
    session: &Session,
    device_id: &str,
) -> Result<String> {
    let secure = secure_token(banner, session, device_id).await?;
    exchange_secure_token(banner, endpoints, &secure).await
}

/// Step one: a single-use code tying the Club Plus session to one banner.
async fn secure_token(banner: Banner, session: &Session, device_id: &str) -> Result<String> {
    // Club Plus, not the banner API: see the note at the top of this module.
    let url = format!("{}/user/token/secure", clubplus_api());
    let mut headers = base_headers(CLUBPLUS_ORIGIN);
    headers.push(("Authorization", format!("Bearer {}", session.access_token)));
    headers.push(("x-device-id", device_id.to_string()));

    let body = serde_json::json!({ "banner": banner.code(), "source": "WEB" });
    let res = fetch::request("POST", &url, &headers, Some(&body.to_string())).await?;

    if res.status == 401 {
        bail!(
            "{} rejected the Club Plus session (401). Run `fsnz auth login` again.",
            banner.name()
        );
    }
    if !res.is_success() {
        bail!(
            "{} returned {} for {url}: {}",
            banner.name(),
            res.status,
            clip(&res.body, 200)
        );
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
        .with_context(|| {
            format!(
                "{} issued no secure_token; the response carried: {}",
                banner.name(),
                field_names(&res.body)
            )
        })
}

/// Step two: swap the code for the token, at the storefront rather than the API.
async fn exchange_secure_token(
    banner: Banner,
    endpoints: &Endpoints,
    secure: &str,
) -> Result<String> {
    let url = format!("{}/api/user/login/sso", endpoints.origin);
    let body = serde_json::json!({
        "key": secure,
        "forceNewSession": false,
        "fingerprintGuest": USER_AGENT,
    });
    let res = fetch::request(
        "POST",
        &url,
        &base_headers(&endpoints.origin),
        Some(&body.to_string()),
    )
    .await?;

    if !res.is_success() {
        bail!(
            "{} would not exchange the login code ({}): {}",
            banner.name(),
            res.status,
            clip(&res.body, 200)
        );
    }
    token_from(&res.body)
        .with_context(|| format!("{} accepted the login but returned no token", banner.name()))
}

/// Pull the banner token out of a response whose exact shape is unconfirmed.
///
/// Rather than guess one field name and fail opaquely, try the plausible ones
/// and -- on a miss -- report the field names that were actually present, which
/// is the one piece of information needed to fix this.
fn token_from(body: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .with_context(|| format!("response was not JSON: {}", clip(body, 200)))?;

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

    bail!(
        "no recognised token field. The response carried: {}",
        field_names(body)
    )
}

/// The field names a response carried, for error messages. Names only: the
/// values are credentials.
fn field_names(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>().join(", "))
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "no object fields".to_string())
}

fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "..."
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
            assert_eq!(token_from(&body).unwrap(), "abc.def.ghi", "key {key}");
        }
    }

    #[test]
    fn an_unknown_shape_reports_the_fields_it_did_see() {
        let body = serde_json::json!({ "sessionThing": "x", "ttl": 60 }).to_string();
        let err = token_from(&body).unwrap_err().to_string();
        assert!(err.contains("sessionThing"), "got: {err}");
        assert!(err.contains("ttl"), "got: {err}");
    }

    #[test]
    fn an_empty_token_does_not_count_as_found() {
        let body = serde_json::json!({ "access_token": "   ", "other": 1 }).to_string();
        assert!(token_from(&body).is_err());
    }

    #[test]
    fn banner_codes_match_the_login_urls() {
        assert_eq!(Banner::NewWorld.code(), "MNW");
        assert_eq!(Banner::PaknSave.code(), "PNS");
    }
}
