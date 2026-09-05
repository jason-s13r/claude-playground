//! Signing in, and getting let in. Two different things.
//!
//! **Signing in** is Auth0's hosted login, walked the way a browser walks it:
//!
//! ```text
//!   GET  auth /authorize            -> 302 to /u/login/identifier?state=...
//!   POST auth /u/login/identifier   -> 302 to /u/login/password?state=...
//!   POST auth /u/login/password     -> 302 to /authorize/resume
//!   GET  auth /authorize/resume     -> 302 to /u/passkey-enrollment (sometimes)
//!   POST auth /u/passkey-enrollment -> 302 back to /authorize/resume
//!   GET  auth /authorize/resume     -> 302 to www /account/login?code=...
//!   POST auth /oauth/token          -> access_token + refresh_token
//! ```
//!
//! **The password submit does not get through, and cannot.** Measured against
//! the live site, step by step:
//!
//! | step | answer |
//! | --- | --- |
//! | `GET /authorize` | the form |
//! | `POST /u/login/identifier` | the next form |
//! | `POST /u/login/password` | **`403`, Akamai "Access Denied"** |
//! | `POST /oauth/token` | Auth0, for real |
//!
//! Akamai guards exactly the one step that would produce a session. It is a
//! `403` with an HTML body, which is indistinguishable from a refused password
//! unless the body is read -- so it is read, and [`crate::Error::Challenged`]
//! is what comes back, because telling someone to check a password that was
//! never seen is the worst answer available.
//!
//! [`login`] is kept anyway. It is correct against Auth0 and only Akamai stops
//! it, so it is one policy change away from working and it is the executable
//! description of the flow. The way in meanwhile is [`refresh`], driven by a
//! refresh token lifted from a browser once -- the token endpoint is not
//! guarded, so from there a session renews itself forever.
//!
//! Two things differ from the Woolworths flow this is otherwise modelled on.
//! Kmart's storefront is a single-page app, so it generates its own PKCE pair
//! rather than having a server do it -- meaning this crate has to, and
//! [`Pkce`] is that. And the flow ends at a `code` on a redirect *to the
//! storefront*, which is behind the bot check: the redirect must be read, not
//! followed, or the code is lost inside a challenge page.

use std::sync::Arc;

use base64::Engine;
use net_kit::wreq;
use sha2::{Digest, Sha256};

use crate::country::Country;
use crate::endpoints::{encode, Endpoints, AUTH_AUDIENCE, AUTH_SCOPE};
use crate::error::{Error, Result};
use crate::http::EMULATION;
use crate::session::{is_admission_cookie, Cookies, Session, Tokens};

/// Narrates each step. The caller supplies it, rather than this crate reading
/// a debug environment variable.
///
/// Nothing passed to it is a secret: the flow's `state` and the authorization
/// code are both dropped, cookies appear by name only, and no credential is
/// ever formatted.
pub type Trace<'a> = &'a (dyn Fn(&str, &str) + Send + Sync);

/// A trace that discards everything.
pub fn no_trace(_step: &str, _detail: &str) {}

/// A PKCE pair: the secret, and the digest of it that goes out first.
///
/// The point of the exchange is that the authorization code is useless without
/// the verifier, so an attacker who intercepts the redirect gets nothing. That
/// only holds if the verifier is unpredictable, which is why this is generated
/// from the system random source and never from anything derived from the
/// clock.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn generate() -> Pkce {
        let mut bytes = [0u8; 32];
        rand::fill(&mut bytes);
        Pkce::from_verifier(b64(&bytes))
    }

    pub fn from_verifier(verifier: String) -> Pkce {
        let challenge = b64(&Sha256::digest(verifier.as_bytes()));
        Pkce {
            verifier,
            challenge,
        }
    }
}

/// Base64url, no padding -- what RFC 7636 specifies and what Auth0 checks.
fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A random URL-safe value, for `state` and `nonce`.
fn nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::fill(&mut bytes);
    b64(&bytes)
}

/// Walk the login flow and come back with tokens.
///
/// **Expect [`crate::Error::Challenged`].** Akamai blocks the password submit,
/// so against the live site this gets as far as the second form and no
/// further. Kept because it is correct against Auth0 and because it documents
/// the flow precisely; see the module docs.
pub async fn login(
    endpoints: &Endpoints,
    email: &str,
    password: &str,
    trace: Trace<'_>,
) -> Result<Tokens> {
    let jar = Arc::new(wreq::cookie::Jar::default());
    let auth_host = host_of(&endpoints.auth);

    // Its own client, not the shared one: this is the one place redirects are
    // followed, and they are followed only within the auth host. The last hop
    // goes to the storefront, which is bot-checked -- following it would trade
    // the authorization code for a challenge page.
    let http = net_kit::http::build(
        net_kit::ClientSpec::new(
            EMULATION,
            wreq::redirect::Policy::custom(move |attempt| {
                if attempt.uri.host() == Some(auth_host.as_str()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }),
        )
        .with_cookies(jar.clone()),
    )
    .map_err(|source| net_kit::HttpError::Transport {
        method: "BUILD",
        url: endpoints.auth.clone(),
        source,
    })?;

    let pkce = Pkce::generate();
    let start = authorize_url(endpoints, &pkce);

    // Step one: Auth0 answers the whole OAuth request with the email form.
    let page = get(&http, "start", &start, trace).await?;
    if page.challenged() {
        return Err(Error::Challenged {
            host: host_of(&endpoints.auth),
        });
    }
    let state = form_state(&page.html).ok_or_else(|| Error::NoSession {
        detail: format!(
            ": the sign-in page at {} carried no login form, so Kmart has changed it",
            endpoints.auth
        ),
    })?;

    // Step two: the email. Auth0 splits identifier and password across two
    // pages and rejects the password form if the identifier one was skipped.
    trace("start", "login form found, submitting the email address");
    let page = post_form(
        &http,
        "identifier",
        &format!(
            "{}/u/login/identifier?state={}",
            endpoints.auth,
            encode(&state)
        ),
        &[
            ("state", state.as_str()),
            ("username", email),
            // The hidden capability fields the page's own script fills in.
            // Auth0 branches on them, so they are sent as a browser would.
            ("js-available", "true"),
            ("webauthn-available", "true"),
            ("is-brave", "false"),
            ("webauthn-platform-available", "true"),
        ],
        trace,
    )
    .await?;

    // A 4xx landing back on the identifier form is Auth0 refusing the email.
    // The re-rendered form carries a `state` of its own, so scraping one is
    // not evidence the step succeeded and this has to be checked first.
    if page.challenged() {
        return Err(Error::Challenged {
            host: host_of(&endpoints.auth),
        });
    }
    if page.bounced(&format!("{}/u/login/identifier", endpoints.auth)) {
        return Err(Error::LoginRefused {
            step: "email address",
            detail: reason(&page.html),
        });
    }
    let state = form_state(&page.html).ok_or_else(|| Error::LoginRefused {
        step: "email address",
        detail: reason(&page.html),
    })?;

    // Step three: the password.
    trace("identifier", "password form found, submitting the password");
    let page = post_form(
        &http,
        "password",
        &format!(
            "{}/u/login/password?state={}",
            endpoints.auth,
            encode(&state)
        ),
        &[
            ("state", state.as_str()),
            ("username", email),
            ("password", password),
            ("js-available", "true"),
            ("webauthn-available", "true"),
            ("is-brave", "false"),
            ("webauthn-platform-available", "true"),
        ],
        trace,
    )
    .await?;
    // Checked before the bounce, because Akamai's denial is also a 4xx on
    // this exact path and would otherwise read as a wrong password.
    if page.challenged() {
        return Err(Error::Challenged {
            host: host_of(&endpoints.auth),
        });
    }
    if page.bounced(&format!("{}/u/login/password", endpoints.auth)) {
        return Err(Error::LoginRefused {
            step: "password",
            detail: reason(&page.html),
        });
    }

    // Step four: Auth0 may interrupt a correct password with an offer to
    // enrol a passkey. There is no way to pre-decline it in the authorize
    // request, so it is declined here -- and it is genuinely optional, which
    // is why aborting it completes the login rather than failing it.
    let page = if page.landed.contains("/u/passkey-enrollment") {
        let state = form_state(&page.html).ok_or_else(|| Error::NoSession {
            detail: ": the passkey prompt carried no state to decline it with".into(),
        })?;
        trace("password", "declining the passkey enrolment prompt");
        post_form(
            &http,
            "passkey",
            &format!(
                "{}/u/passkey-enrollment?state={}",
                endpoints.auth,
                encode(&state)
            ),
            &[
                ("state", state.as_str()),
                ("action", "abort-passkey-enrollment"),
            ],
            trace,
        )
        .await?
    } else {
        page
    };

    // Step five: the redirect chain has stopped at the storefront, because the
    // policy above refuses to leave the auth host. The code is in that URL.
    let code = code_from(&page.landed).ok_or_else(|| Error::NoSession {
        detail: format!(
            ": the flow ended at {} without an authorization code{}",
            page.landed.split('?').next().unwrap_or(&page.landed),
            reason(&page.html)
        ),
    })?;

    trace("passkey", "exchanging the authorization code for a token");
    exchange(&http, endpoints, &code, &pkce.verifier, trace).await
}

/// The `/authorize` URL that starts it all.
fn authorize_url(endpoints: &Endpoints, pkce: &Pkce) -> String {
    let params = [
        ("client_id", endpoints.auth_client_id.as_str()),
        ("audience", AUTH_AUDIENCE),
        ("redirect_uri", endpoints.auth_redirect.as_str()),
        ("scope", AUTH_SCOPE),
        ("response_type", "code"),
        ("response_mode", "query"),
        // Ask for the form even when a session cookie is present, so the
        // result does not depend on state this process cannot see.
        ("prompt", "login"),
        ("code_challenge", &pkce.challenge),
        ("code_challenge_method", "S256"),
        ("state", &nonce()),
        ("nonce", &nonce()),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}?{query}", endpoints.authorize())
}

/// Trade the authorization code for tokens.
async fn exchange(
    http: &wreq::Client,
    endpoints: &Endpoints,
    code: &str,
    verifier: &str,
    trace: Trace<'_>,
) -> Result<Tokens> {
    let body = serde_json::json!({
        "client_id": endpoints.auth_client_id,
        "code_verifier": verifier,
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": endpoints.auth_redirect,
    });
    token_request(http, endpoints, body, "authorization code", trace).await
}

/// Renew from a refresh token, which is what `offline_access` bought.
///
/// The reason a fifteen-minute token is workable at all: no password, no
/// forms, one request.
///
/// **Auth0 rotates these.** The client is a browser app and therefore public,
/// so every successful call invalidates the token it was given and returns a
/// replacement -- which the caller has to keep, or the session ends fifteen
/// minutes later. [`crate::Client::renew`] does.
///
/// The same rotation is why a token copied out of a browser has a short shelf
/// life: the tab it came from goes on refreshing and spends it. A rejection
/// here usually means that rather than a malformed token.
pub async fn refresh(
    endpoints: &Endpoints,
    refresh_token: &str,
    trace: Trace<'_>,
) -> Result<Tokens> {
    let http = net_kit::http::build(net_kit::ClientSpec::new(
        EMULATION,
        wreq::redirect::Policy::none(),
    ))
    .map_err(|source| net_kit::HttpError::Transport {
        method: "BUILD",
        url: endpoints.token(),
        source,
    })?;
    let body = serde_json::json!({
        "client_id": endpoints.auth_client_id,
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
    });
    token_request(&http, endpoints, body, "refresh token", trace).await
}

async fn token_request(
    http: &wreq::Client,
    endpoints: &Endpoints,
    body: serde_json::Value,
    what: &'static str,
    trace: Trace<'_>,
) -> Result<Tokens> {
    let url = endpoints.token();
    let res = http
        .post(&url)
        .header(wreq::header::CONTENT_TYPE, "application/json")
        .header(wreq::header::ORIGIN, Country::Nz.origin())
        .body(body.to_string())
        .send()
        .await
        .map_err(|source| net_kit::HttpError::Transport {
            method: "POST",
            url: url.clone(),
            source,
        })?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    trace("token", &format!("{status} from the token endpoint"));

    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    if let Some(access) = json.get("access_token").and_then(|v| v.as_str()) {
        return Ok(Tokens::new(
            access.to_string(),
            json.get("refresh_token")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            json.get("expires_in").and_then(|v| v.as_u64()),
        ));
    }

    // Auth0 states its refusals in the body, and the status alone is
    // misleading: a rejected code and a bot challenge are both 403.
    if crate::error::is_challenge_body(&text) {
        return Err(Error::Challenged {
            host: host_of(&endpoints.auth),
        });
    }
    let detail = json
        .get("error_description")
        .or_else(|| json.get("error"))
        .and_then(|v| v.as_str())
        .map(|m| format!(": {m}"))
        .unwrap_or_default();
    Err(Error::LoginRefused { step: what, detail })
}

/// Pull the Akamai admission cookies out of a Netscape-format `cookies.txt`,
/// which is what browser export extensions and `curl -c` write.
///
/// **This is not a sign-in and it is not optional.** The gateway is behind
/// Akamai Bot Manager, which serves a sensor-script interstitial to any client
/// that has not run the script -- there is no fingerprint or warm-up that gets
/// past it, so the only cookies that work are ones a real browser earned. They
/// last about a day.
///
/// Only the bot-manager cookies are taken; see
/// [`crate::session::is_admission_cookie`]. An export is a whole browser
/// profile and the rest of it has no business in a credential store.
pub fn from_netscape(text: &str, country: Country) -> Cookies {
    net_kit::cookies::from_netscape(text, country.cookie_domain())
        .into_iter()
        .filter(|(name, value)| is_admission_cookie(name) && !value.is_empty())
        .collect()
}

/// Both countries at once, since one export usually carries both.
pub fn session_from_netscape(text: &str) -> Session {
    let mut session = Session::new();
    for country in Country::ALL {
        let cookies = from_netscape(text, country);
        if !cookies.is_empty() {
            session = session.with_cookies(country, cookies);
        }
    }
    session
}

// ------------------------------------------------------------------ plumbing

/// One response from the login flow: where it landed and what it served.
struct Page {
    /// The URL the redirect chain ended on, which is what says whether a step
    /// moved forward or bounced back to its own form.
    landed: String,
    rejected: bool,
    html: String,
}

impl Page {
    /// Whether this page is the same form that was just submitted, which is
    /// how Auth0 reports a refusal: a 4xx re-rendering the form with a banner.
    fn bounced(&self, submitted_to: &str) -> bool {
        let path = |u: &str| {
            url::Url::parse(u)
                .map(|p| p.path().to_string())
                .unwrap_or_default()
        };
        self.rejected && path(&self.landed) == path(submitted_to)
    }

    fn challenged(&self) -> bool {
        crate::error::is_challenge_body(&self.html)
    }
}

async fn get(http: &wreq::Client, step: &str, url: &str, trace: Trace<'_>) -> Result<Page> {
    // No headers set on purpose: the emulation already sends the set a real
    // Firefox sends for a navigation.
    let res = http.get(url).send().await;
    body(res, step, url, trace).await
}

async fn post_form(
    http: &wreq::Client,
    step: &str,
    url: &str,
    form: &[(&str, &str)],
    trace: Trace<'_>,
) -> Result<Page> {
    let res = http
        .post(url)
        // Only the two a form post adds over the emulation's own headers.
        .header(wreq::header::ORIGIN, origin_of(url))
        .header(wreq::header::REFERER, url)
        .form(form)
        .send()
        .await;
    body(res, step, url, trace).await
}

async fn body(
    res: std::result::Result<wreq::Response, wreq::Error>,
    step: &str,
    url: &str,
    trace: Trace<'_>,
) -> Result<Page> {
    let res = res.map_err(|source| net_kit::HttpError::Transport {
        method: "GET",
        url: url.to_string(),
        source,
    })?;
    let status = res.status();
    // Where the redirect chain actually ended, which is the single most useful
    // fact when the flow goes somewhere unexpected -- and, on the last step,
    // is where the authorization code is.
    //
    // A redirect the policy refused to follow is handed back as the 30x
    // itself, so its `Location` is the destination and the response URI is
    // still the request before it. That last unfollowed hop is exactly the
    // one carrying the code, which is why the header is preferred.
    let landed = res
        .headers()
        .get(wreq::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|loc| absolute(loc, &res.uri().to_string()))
        .unwrap_or_else(|| res.uri().to_string());
    let html = res.text().await.unwrap_or_default();
    trace(
        step,
        &format!(
            "{status} -> {} ({} bytes){}",
            // The query carries the flow's state and, at the end, the code.
            landed.split('?').next().unwrap_or(&landed),
            html.len(),
            match error_message(&html) {
                Some(m) => format!(" banner: {m:?}"),
                None => String::new(),
            }
        ),
    );
    // A rejected form comes back as a 4xx carrying the page and its error
    // message, which is more useful than the status alone -- so 4xx is
    // recorded rather than raised, and the caller decides what a bounce means.
    if status.is_server_error() {
        return Err(net_kit::HttpError::Status {
            method: "POST",
            url: url.to_string(),
            status: status.as_u16(),
            detail: String::new(),
            body: String::new(),
        }
        .into());
    }
    Ok(Page {
        landed,
        rejected: status.is_client_error(),
        html,
    })
}

/// The `code` query parameter off the URL the flow ended on.
fn code_from(landed: &str) -> Option<String> {
    let url = url::Url::parse(landed).ok()?;
    url.query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .filter(|c| !c.is_empty())
}

/// An Auth0 banner as a trailing clause, or nothing when the page carried none.
fn reason(html: &str) -> String {
    error_message(html)
        .map(|m| format!(": {m}"))
        .unwrap_or_default()
}

fn origin_of(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => u.origin().ascii_serialization(),
        Err(_) => url.to_string(),
    }
}

/// A `Location` made absolute against the request it answered. Auth0 sends
/// relative ones between its own steps and an absolute one at the end.
fn absolute(location: &str, base: &str) -> String {
    match url::Url::parse(location) {
        Ok(_) => location.to_string(),
        Err(_) => url::Url::parse(base)
            .and_then(|b| b.join(location))
            .map(String::from)
            .unwrap_or_else(|_| location.to_string()),
    }
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| url.to_string())
}

/// The value of the login form's hidden `state` field.
///
/// Written by hand rather than with an HTML parser: one attribute on one
/// input, on a page this crate has no other reason to understand.
fn form_state(html: &str) -> Option<String> {
    let mut rest = html;
    while let Some(at) = rest.find("name=\"state\"") {
        // The value may sit either side of the name on the same tag, so the
        // search is bounded to the tag rather than run forward from the name.
        let tag_start = rest[..at].rfind('<')?;
        let tag_end = rest[at..].find('>').map(|e| at + e)?;
        let tag = &rest[tag_start..tag_end];
        if let Some(value) = attribute(tag, "value") {
            if !value.is_empty() {
                return Some(value);
            }
        }
        rest = &rest[tag_end..];
    }
    None
}

/// An Auth0 error banner, which is the page's own explanation of a refusal.
///
/// Auth0 renders a refusal several ways depending on which field was at fault
/// and which template version is live, so several markers are tried.
fn error_message(html: &str) -> Option<String> {
    const MARKERS: [&str; 5] = [
        "id=\"error-element-password\"",
        "id=\"error-element-username\"",
        "class=\"ulp-input-error-message\"",
        "class=\"ulp-error-info\"",
        "role=\"alert\"",
    ];
    MARKERS
        .iter()
        .filter_map(|marker| html.find(marker).map(|at| at + marker.len()))
        .filter_map(|at| {
            let after = html[at..].find('>').map(|e| at + e + 1)?;
            let end = html[after..].find('<').map(|e| after + e)?;
            let text = strip_entities(html[after..end].trim());
            (!text.is_empty()).then_some(text)
        })
        .next()
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let at = tag.find(&needle)? + needle.len();
    let end = tag[at..].find('"').map(|e| at + e)?;
    Some(strip_entities(&tag[at..end]))
}

/// The handful of entities that turn up in these pages' text and attributes.
fn strip_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pkce_challenge_matches_the_rfc_7636_worked_example() {
        // Appendix B of RFC 7636. If this drifts, Auth0 rejects every login
        // with an opaque `invalid_grant` and nothing else says why.
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into());
        assert_eq!(
            pkce.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_generated_verifier_is_random_and_the_right_length() {
        let a = Pkce::generate();
        let b = Pkce::generate();
        assert_ne!(a.verifier, b.verifier, "a fixed verifier defeats the point");
        // RFC 7636 requires 43..=128 characters; 32 bytes base64url is 43.
        assert_eq!(a.verifier.len(), 43);
        assert!(!a.verifier.contains('='), "no padding");
        assert!(
            !a.verifier.contains('+') && !a.verifier.contains('/'),
            "url-safe"
        );
    }

    #[test]
    fn the_authorize_url_carries_everything_auth0_binds_the_code_to() {
        let pkce = Pkce::from_verifier("v".into());
        let url = authorize_url(&Endpoints::of(Country::Nz), &pkce);
        assert!(url.starts_with("https://auth.kmart.com.au/authorize?"));
        for needle in [
            &format!("client_id={}", Endpoints::of(Country::Nz).auth_client_id) as &str,
            "code_challenge_method=S256",
            "response_type=code",
            "prompt=login",
            "offline_access",
        ] {
            assert!(url.contains(needle), "missing {needle} in {url}");
        }
        assert!(url.contains(&format!("code_challenge={}", pkce.challenge)));
    }

    #[test]
    fn the_australian_flow_shares_the_tenant_but_not_the_application() {
        // One Auth0 tenant issues for both countries -- which is why a single
        // account works on either -- but each storefront is a separate
        // registered application. This test asserted the two flows were
        // identical until the Australian home page was read rather than
        // assumed to match, and they are not: a session minted under one
        // client id cannot be renewed under the other.
        let pkce = Pkce::from_verifier("v".into());
        let au = authorize_url(&Endpoints::of(Country::Au), &pkce);
        let nz = authorize_url(&Endpoints::of(Country::Nz), &pkce);

        for url in [&au, &nz] {
            assert!(url.starts_with("https://auth.kmart.com.au/authorize?"));
            assert!(url.contains("audience=https%3A%2F%2Fapi.kmart.com.au"));
        }
        assert!(au.contains(&format!(
            "client_id={}",
            Endpoints::of(Country::Au).auth_client_id
        )));
        assert_ne!(
            Endpoints::of(Country::Au).auth_client_id,
            Endpoints::of(Country::Nz).auth_client_id
        );
        assert!(au.contains("redirect_uri=https%3A%2F%2Fwww.kmart.com.au"));
        assert!(nz.contains("redirect_uri=https%3A%2F%2Fwww.kmart.co.nz"));
    }

    #[test]
    fn each_login_gets_a_fresh_state_and_nonce() {
        // Replaying either is what a fixed value would allow.
        let pkce = Pkce::from_verifier("v".into());
        let e = Endpoints::of(Country::Nz);
        assert_ne!(authorize_url(&e, &pkce), authorize_url(&e, &pkce));
    }

    #[test]
    fn the_state_is_read_off_the_hidden_field() {
        let html = r#"<form><input type="hidden" name="state" value="hKFo2SB-abc"/>
                      <input name="username" type="email"/></form>"#;
        assert_eq!(form_state(html).as_deref(), Some("hKFo2SB-abc"));
    }

    #[test]
    fn the_value_is_found_even_when_it_precedes_the_name() {
        let html = r#"<input value="abc123" type="hidden" name="state">"#;
        assert_eq!(form_state(html).as_deref(), Some("abc123"));
    }

    #[test]
    fn an_empty_state_is_skipped_for_a_later_one() {
        let html = r#"<input name="state" value=""><input name="state" value="real">"#;
        assert_eq!(form_state(html).as_deref(), Some("real"));
    }

    #[test]
    fn the_code_is_read_off_the_url_the_flow_stopped_at() {
        // The chain stops at the storefront rather than following into it,
        // because that host is bot-checked.
        let landed = "https://www.kmart.co.nz/account/login?code=IvHJ-abc&state=xyz";
        assert_eq!(code_from(landed).as_deref(), Some("IvHJ-abc"));
        // An error landing has no code, and that is the interesting failure.
        assert_eq!(
            code_from("https://www.kmart.co.nz/account/login?error=login_required"),
            None
        );
        assert_eq!(code_from("not a url"), None);
    }

    #[test]
    fn a_relative_location_is_resolved_against_the_step_that_sent_it() {
        assert_eq!(
            absolute(
                "/u/login/password?state=x",
                "https://auth.kmart.com.au/u/login/identifier"
            ),
            "https://auth.kmart.com.au/u/login/password?state=x"
        );
        // The final hop is absolute and must survive untouched.
        let done = "https://www.kmart.co.nz/account/login?code=abc";
        assert_eq!(
            absolute(done, "https://auth.kmart.com.au/authorize/resume"),
            done
        );
    }

    #[test]
    fn a_bounce_is_the_same_path_with_a_4xx() {
        let page = Page {
            landed: "https://auth.kmart.com.au/u/login/password?state=x".into(),
            rejected: true,
            html: String::new(),
        };
        assert!(page.bounced("https://auth.kmart.com.au/u/login/password"));
        // Moving on is not a bounce even when the status is odd.
        assert!(!page.bounced("https://auth.kmart.com.au/u/login/identifier"));
    }

    #[test]
    fn an_auth0_banner_becomes_the_reason_a_login_was_refused() {
        let html = r#"<span id="error-element-password" class="ulp-input-error-message">
            Wrong email or password</span>"#;
        assert_eq!(reason(html), ": Wrong email or password");
        assert_eq!(reason("<p>fine</p>"), "");
    }

    #[test]
    fn an_export_yields_only_the_cookies_that_matter_and_only_for_that_country() {
        let text = "\
# Netscape HTTP Cookie File
.kmart.co.nz\tTRUE\t/\tTRUE\t9999999999\t_abck\tnz-admission
.kmart.co.nz\tTRUE\t/\tTRUE\t9999999999\tbm_sz\tnz-bm
.kmart.co.nz\tTRUE\t/\tTRUE\t9999999999\tutag_main_v_id\ttracking
.kmart.com.au\tTRUE\t/\tTRUE\t9999999999\t_abck\tau-admission
.example.test\tTRUE\t/\tTRUE\t9999999999\t_abck\tsomeone-else
";
        let nz = from_netscape(text, Country::Nz);
        assert_eq!(nz.get("_abck").map(String::as_str), Some("nz-admission"));
        assert!(nz.contains_key("bm_sz"));
        assert!(
            !nz.contains_key("utag_main_v_id"),
            "analytics is not a credential"
        );
        assert_eq!(nz.len(), 2);

        let au = from_netscape(text, Country::Au);
        assert_eq!(au.get("_abck").map(String::as_str), Some("au-admission"));

        // One export usually carries both countries.
        let session = session_from_netscape(text);
        assert!(session.admitted(Country::Nz));
        assert!(session.admitted(Country::Au));
        assert!(!session.signed_in(), "cookies are not a sign-in");
    }

    #[test]
    fn an_export_with_nothing_relevant_yields_nothing() {
        let session = session_from_netscape("# empty\n");
        assert!(!session.admitted(Country::Nz));
        assert!(!session.admitted(Country::Au));
    }
}
