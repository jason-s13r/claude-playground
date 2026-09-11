//! Signing in, and staying signed in.
//!
//! The shape of this module is decided by one measured fact: **`accounts.login`
//! refuses any request without a reCAPTCHA token, and refuses it before it
//! looks at the password.** Sending a correct email and password with no
//! `captchaToken` answers `400006, "Invalid CaptchaType / Invalid
//! CaptchaToken"` -- so this is policy rather than risk-based escalation, and
//! there is no headless username-and-password path to find.
//!
//! Everything *after* that is unguarded. Gigya's own SDK marks which of its
//! methods carry a risk assessment, and `accounts.getAccountInfo` is not among
//! them; nor does any GraphQL operation on either storefront send the
//! `X-ReCaptcha` header its bundle can produce. So the split is:
//!
//! ```text
//!   once, with a browser   password + captchaToken -> accounts.login -> login_token
//!   every time after       login_token -> accounts.getAccountInfo -> UIDSignature
//!                          UIDSignature -> loginGigya -> a 120-minute Magento JWT
//! ```
//!
//! Which is why [`login`] takes a captcha token rather than minting one: a
//! browser is the app's business, and a library that shelled out to one would
//! be untestable and unwelcome in the two-thirds of commands that need no
//! account at all.
//!
//! The signature is re-fetched on every exchange rather than stored, because it
//! is an HMAC over a timestamp and goes stale in minutes. The `login_token` is
//! the only durable half -- and it is also the value of the site's own
//! `glt_<apiKey>` cookie, which is what makes pasting one a reasonable
//! alternative to driving a browser at all.

use net_kit::wreq;
use serde::Serialize;

use crate::banner::{Banner, Endpoints};
use crate::error::{Error, Result, GIGYA_INVALID_CREDENTIALS, GIGYA_INVALID_SESSION};
use crate::wire::GigyaResponse;

/// Gigya's captcha type name for the invisible reCAPTCHA these sites use.
pub const CAPTCHA_TYPE: &str = "reCaptchaV3";

/// How long a Gigya session should be asked for, in seconds.
///
/// The website sends `sessionExpiration=0`, which means "until the browser is
/// closed" -- sensible for a tab, useless for a tool that will be run again
/// tomorrow. Gigya honours the parameter on `accounts.login`, so asking for a
/// year is the one thing this crate does differently from the site, and it is
/// deliberate: it is what keeps the browser a once-only cost.
///
/// A site policy may cap it lower. That is fine and invisible -- the token
/// simply lapses sooner and the app says to sign in again.
pub const SESSION_SECONDS: i64 = 365 * 24 * 60 * 60;

/// An identity assertion, fresh enough for Magento to accept.
#[derive(Clone, Debug, Serialize)]
pub struct Assertion {
    /// The numeric SAP customer id. These sites use custom UIDs, so this is
    /// Gigya's `loginProviderUID` rather than a GUID.
    pub uid: String,
    pub uid_signature: String,
    pub signature_timestamp: String,
    pub email: String,
}

/// What a completed sign-in yields.
#[derive(Clone, Debug)]
pub struct Login {
    /// The credential to keep. Everything else here can be re-derived from it.
    pub login_token: String,
    pub assertion: Assertion,
}

/// Exchange a password for a Gigya session. **Needs a captcha token.**
///
/// `captcha` is a reCAPTCHA v3 token minted on the fascia's own origin -- the
/// token is bound to both the site key and the page it was executed on, so one
/// earned anywhere else will be refused. Getting one means running Google's
/// script in a browser; see the app's `auth login`.
pub async fn login(
    http: &wreq::Client,
    endpoints: &Endpoints,
    banner: Banner,
    api_key: &str,
    email: &str,
    password: &str,
    captcha: &str,
) -> Result<Login> {
    let expiration = SESSION_SECONDS.to_string();
    let form = [
        ("loginID", email),
        ("password", password),
        ("APIKey", api_key),
        ("captchaToken", captcha),
        ("captchaType", CAPTCHA_TYPE),
        ("sessionExpiration", expiration.as_str()),
        ("targetEnv", "jssdk"),
        ("loginMode", "standard"),
        ("authMode", "cookie"),
        ("includeUserInfo", "true"),
        ("include", "profile,data"),
        ("format", "json"),
    ];

    let answer = call(http, endpoints, "accounts.login", &form, banner).await?;

    let login_token = answer
        .session_info
        .as_ref()
        .and_then(|s| s.login_token.clone())
        .ok_or_else(|| {
            Error::Shape("Gigya accepted the sign-in but returned no session token".into())
        })?;

    Ok(Login {
        assertion: assertion(&answer, email)?,
        login_token,
    })
}

/// A fresh identity assertion from a stored session. **No captcha.**
///
/// The whole refresh path in one call: the signature Magento verifies is an
/// HMAC over the UID and a timestamp, so it has to be minted per exchange, and
/// this is the unguarded method that mints one.
pub async fn assert(
    http: &wreq::Client,
    endpoints: &Endpoints,
    banner: Banner,
    api_key: &str,
    login_token: &str,
    email: Option<&str>,
) -> Result<Assertion> {
    let form = [
        ("APIKey", api_key),
        ("login_token", login_token),
        ("include", "profile,data"),
        ("format", "json"),
    ];
    let answer = call(http, endpoints, "accounts.getAccountInfo", &form, banner).await?;
    let email = email
        .map(str::to_string)
        .or_else(|| answer.profile.as_ref().and_then(|p| p.email.clone()))
        .ok_or_else(|| {
            Error::Shape("Gigya did not say which address this account signs in with".into())
        })?;
    assertion(&answer, &email)
}

fn assertion(answer: &GigyaResponse, email: &str) -> Result<Assertion> {
    Ok(Assertion {
        uid: answer
            .uid
            .clone()
            .ok_or_else(|| Error::Shape("Gigya returned no UID".into()))?,
        uid_signature: answer
            .uid_signature
            .clone()
            .ok_or_else(|| Error::Shape("Gigya returned no UIDSignature".into()))?,
        signature_timestamp: answer
            .signature_timestamp
            .clone()
            .ok_or_else(|| Error::Shape("Gigya returned no signatureTimestamp".into()))?,
        email: email.to_string(),
    })
}

/// One Gigya REST call, with its errors read out of the body.
///
/// Gigya answers `200 OK` for everything and puts the verdict in `errorCode`,
/// so a status check alone would call every failure a success.
async fn call(
    http: &wreq::Client,
    endpoints: &Endpoints,
    method: &'static str,
    form: &[(&str, &str)],
    banner: Banner,
) -> Result<GigyaResponse> {
    let url = endpoints.gigya(method);
    let (_, body) = net_kit::http::text(
        "POST",
        &url,
        http.post(&url)
            .header(
                wreq::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(form_urlencoded(form))
            .send()
            .await,
    )
    .await?;

    let answer: GigyaResponse = serde_json::from_str(&body)
        .map_err(|e| Error::decode(format!("reading the answer to {method}"), e))?;

    if answer.error_code == 0 {
        return Ok(answer);
    }

    let detail = answer.error_details.clone();
    let message = answer
        .error_message
        .clone()
        .unwrap_or_else(|| "no reason given".into());

    // The one failure worth naming apart from the rest: it is not a credential
    // problem, and no amount of retyping a password fixes it.
    if detail
        .as_deref()
        .is_some_and(|d| d.to_ascii_lowercase().contains("captcha"))
    {
        return Err(Error::CaptchaRequired {
            banner: banner.name(),
        });
    }
    if answer.error_code == GIGYA_INVALID_SESSION {
        return Err(Error::LoginLapsed {
            banner: banner.name(),
        });
    }

    Err(Error::Gigya {
        method,
        code: answer.error_code,
        message,
        detail,
    })
}

/// Whether a Gigya failure means the password was wrong, as opposed to anything
/// else having gone wrong. Worth telling apart: one is the person's to fix.
pub fn is_bad_credentials(error: &Error) -> bool {
    matches!(error, Error::Gigya { code, .. } if *code == GIGYA_INVALID_CREDENTIALS)
}

/// `a=1&b=2`, percent-encoded.
///
/// Hand-rolled rather than pulling in a form encoder: the captcha token runs to
/// two and a half kilobytes of base64 with `-` and `_` in it, and the only
/// thing that matters is that those survive unmangled.
fn form_urlencoded(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_captcha_token_survives_encoding_intact() {
        // reCAPTCHA v3 tokens are base64url and run to kilobytes; mangling one
        // character makes the whole sign-in fail for no visible reason.
        let token = "0cAFcWeA78qW1d8iBdElYTBe4A0-TnJGD2O5GYgkcG1_XBupP3XmQRzDkd9XRot5Q01g6B47FWK83";
        let body = form_urlencoded(&[("captchaToken", token)]);
        assert_eq!(body, format!("captchaToken={token}"));
    }

    #[test]
    fn a_password_is_encoded_rather_than_sent_raw() {
        let body = form_urlencoded(&[("password", "p@ss word&more")]);
        assert_eq!(body, "password=p%40ss%20word%26more");
    }

    #[test]
    fn a_session_is_asked_to_outlive_the_browser_tab_the_site_assumes() {
        // The site sends 0, meaning "until this tab closes". A tool run again
        // tomorrow needs the opposite, and this is the parameter that buys it.
        let form = form_urlencoded(&[("sessionExpiration", &SESSION_SECONDS.to_string())]);
        assert_eq!(form, "sessionExpiration=31536000");
    }

    #[test]
    fn bad_credentials_are_told_apart_from_everything_else() {
        let wrong = Error::Gigya {
            method: "accounts.login",
            code: GIGYA_INVALID_CREDENTIALS,
            message: "Invalid LoginID".into(),
            detail: None,
        };
        assert!(is_bad_credentials(&wrong));

        let other = Error::Gigya {
            method: "accounts.login",
            code: 500_001,
            message: "General Server Error".into(),
            detail: None,
        };
        assert!(!is_bad_credentials(&other));
        assert!(!is_bad_credentials(&Error::CaptchaRequired {
            banner: "Briscoes"
        }));
    }
}
