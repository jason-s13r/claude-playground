//! Signing in.
//!
//! Woolworths delegates login to Auth0's hosted pages, and the session it ends
//! up with is an encrypted cookie only the storefront's own server can read.
//! There is no token endpoint to call: the only way to obtain a session is to
//! walk the same redirect chain a browser does.
//!
//! ```text
//!   GET  www /auth/login          -> 307 to auth /authorize
//!   GET  auth /authorize          -> 302 to /u/login/identifier?state=...
//!   POST auth /u/login/identifier -> 302 to /u/login/password?state=...
//!   POST auth /u/login/password   -> 302 to /authorize/resume
//!   GET  auth /authorize/resume   -> 302 to www /auth/callback?code=...
//!   GET  www /auth/callback       -> 307, and sets __session__0/__session__1
//! ```
//!
//! Two things make this tractable. The storefront starts the flow itself, so
//! the PKCE challenge, `state` and `nonce` are all generated server-side and
//! never have to be computed here. And each Auth0 form echoes the flow's
//! `state` in a hidden field, so following it is a matter of scraping one value
//! and posting it back.
//!
//! The auth host sits behind Akamai Bot Manager, and getting through it is
//! entirely a matter of the TLS handshake. With a stock rustls client the
//! identifier step is answered with a bare `400` and no explanation; with the
//! browser handshake [`crate::session::EMULATION`] selects, the same requests
//! are answered normally. Akamai's JavaScript sensor never has to run --
//! `ak_bmsc` and the rest of the bot-manager cookies are issued on an ordinary
//! page load, provided the handshake looks right.
//!
//! It is still somebody else's login page, and a change to it will break this.
//! [`Session`] can also be filled in from cookies exported from a browser --
//! `wwnz auth import` -- which is the way back in when that happens.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::api::Endpoints;
use crate::session::{Session, EMULATION, SESSION_COOKIE_PREFIX};

/// Follow the login flow and return the cookies it ends with.
pub async fn login(endpoints: &Endpoints, email: &str, password: &str) -> Result<Session> {
    let jar = Arc::new(wreq::cookie::Jar::default());
    let http = wreq::Client::builder()
        .emulation(EMULATION)
        .cookie_provider(jar.clone())
        // The chain is six redirects across two hosts, and wreq follows none of
        // them by default. Twenty rather than ten because a login that loops is
        // better reported as a loop than as a mystery.
        .redirect(wreq::redirect::Policy::limited(20))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("building the login client")?;

    // Step one: the storefront hands the whole OAuth request to Auth0, which
    // answers with the email form.
    let start = format!("{}/auth/login?returnTo=/", endpoints.origin);
    let page = get(&http, "start", &start).await?;
    let state = form_state(&page.html).with_context(|| {
        format!(
            "the sign-in page at {start} did not carry a login form. \
             Woolworths may have changed it, or be serving a bot check; \
             `wwnz auth import` takes cookies from a browser instead."
        )
    })?;

    // Step two: the email. Auth0 splits identifier and password across two
    // pages, and rejects the password form if the identifier one was skipped.
    trace("start", "login form found, submitting the email address");
    let page = post_form(
        &http,
        "identifier",
        &format!("{}/u/login/identifier?state={state}", endpoints.auth),
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
    )
    .await?;

    // A 4xx landing back on the identifier form is Auth0 refusing the email.
    // The re-rendered form carries a `state` of its own, so scraping one is not
    // evidence the step succeeded and this has to be checked first.
    let identifier_url = format!("{}/u/login/identifier", endpoints.auth);
    if page.bounced(&identifier_url) {
        bail!(
            "the sign-in page refused the email address{}\n\
             The account may not exist. If it does, the sign-in page has \
             probably changed, or Akamai is refusing this client: sign in with a \
             browser, export its cookies, and use `wwnz auth import` instead.\n\
             WWNZ_DEBUG_AUTH=1 narrates each step of the flow.",
            reason(&page.html)
        );
    }

    // The password page carries a fresh state; reusing the first one fails.
    let state = form_state(&page.html).with_context(|| {
        format!(
            "the password form did not appear after submitting the email address{}",
            reason(&page.html)
        )
    })?;

    // Step three: the password. On success Auth0 redirects back through
    // /authorize/resume to the storefront's callback, which is what sets the
    // session cookie; the client follows all of that.
    trace("identifier", "password form found, submitting the password");
    let page = post_form(
        &http,
        "password",
        &format!("{}/u/login/password?state={state}", endpoints.auth),
        &[
            ("state", state.as_str()),
            ("username", email),
            ("password", password),
        ],
    )
    .await?;

    let password_url = format!("{}/u/login/password", endpoints.auth);
    if page.bounced(&password_url) {
        bail!(
            "Woolworths rejected the password{}\n\
             Check it, or sign in with a browser and use `wwnz auth import`.",
            reason(&page.html)
        );
    }

    let cookies = jar_cookies(&jar, &endpoints.origin)?;
    trace(
        "password",
        &format!(
            "cookies on {}: [{}]; on {}: [{}]",
            endpoints.origin,
            names(&cookies),
            endpoints.auth,
            names(&jar_cookies(&jar, &endpoints.auth)?),
        ),
    );
    let session = Session::from_cookies(cookies);
    if !session.account {
        // Landing back on a form means Auth0 rejected something rather than
        // completing the flow, and the page says what.
        bail!(
            "signing in did not produce a session{}\n\
             Check the email and password. If they are right, Woolworths may have \
             added a step this cannot follow (a verification code, or a bot check); \
             sign in with a browser and use `wwnz auth import` instead.",
            reason(&page.html)
        );
    }
    Ok(session)
}

/// One response from the login flow: where it landed and what it served.
struct Page {
    /// The URL the redirect chain ended on, which is what says whether a step
    /// moved forward or bounced back to its own form.
    landed: String,
    rejected: bool,
    html: String,
}

impl Page {
    /// Whether this page is the same form that was just submitted, which is how
    /// Auth0 reports a refusal: a 4xx re-rendering the form with a banner.
    fn bounced(&self, submitted_to: &str) -> bool {
        let path = |u: &str| {
            url::Url::parse(u)
                .map(|p| p.path().to_string())
                .unwrap_or_default()
        };
        self.rejected && path(&self.landed) == path(submitted_to)
    }
}

async fn get(http: &wreq::Client, step: &str, url: &str) -> Result<Page> {
    let res = http
        // No headers are set here on purpose: the emulation already sends the
        // set a real Firefox sends for a navigation, and overriding parts of it
        // by hand is how the set stops being self-consistent.
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    body(res, step, url).await
}

async fn post_form(
    http: &wreq::Client,
    step: &str,
    url: &str,
    form: &[(&str, &str)],
) -> Result<Page> {
    let res = http
        .post(url)
        // Only the two a form post adds over the emulation's own headers.
        .header(wreq::header::ORIGIN, origin_of(url))
        .header(wreq::header::REFERER, url)
        .form(form)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    body(res, step, url).await
}

async fn body(res: wreq::Response, step: &str, url: &str) -> Result<Page> {
    let status = res.status();
    // Where the redirect chain actually ended, which is the single most useful
    // fact when the flow goes somewhere unexpected.
    let landed = res.uri().to_string();
    let landed_path = url::Url::parse(&landed)
        .map(|url| format!("{}{}", url.origin().ascii_serialization(), url.path()))
        .unwrap_or_else(|_| landed.clone());
    let html = res.text().await.unwrap_or_default();
    trace(
        step,
        &format!(
            "{status} -> {} ({} bytes){}",
            landed_path,
            html.len(),
            match error_message(&html) {
                Some(m) => format!(" banner: {m:?}"),
                None => String::new(),
            }
        ),
    );
    // A rejected form comes back as a 4xx carrying the page and its error
    // message, which is more useful than the status alone -- so 4xx is recorded
    // rather than raised, and the caller decides what a bounce means.
    if status.is_server_error() {
        bail!("{url} returned {status}");
    }
    Ok(Page {
        landed: landed.to_string(),
        rejected: status.is_client_error(),
        html,
    })
}

/// Narrate the login flow to stderr when `WWNZ_DEBUG_AUTH` is set.
///
/// This walks somebody else's login pages, so when it breaks the question is
/// always "which step, and where did it land?". Nothing printed here is a
/// secret: query strings are dropped (they carry the flow's `state` token),
/// cookies appear by name only, and no credential is ever formatted.
fn trace(step: &str, detail: &str) {
    if std::env::var_os("WWNZ_DEBUG_AUTH").is_some() {
        eprintln!("auth: {step}: {detail}");
    }
}

/// An Auth0 banner as a trailing clause, or nothing when the page carried none.
fn reason(html: &str) -> String {
    error_message(html)
        .map(|m| format!(": {m}"))
        .unwrap_or_default()
}

/// Cookie names only. Their values are credentials and never get printed.
fn names(cookies: &BTreeMap<String, String>) -> String {
    cookies.keys().cloned().collect::<Vec<_>>().join(", ")
}

fn origin_of(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => u.origin().ascii_serialization(),
        Err(_) => url.to_string(),
    }
}

/// The cookies the jar holds for the storefront, which is where the session
/// lands. Auth0's own cookies stay on its host and are of no further use.
fn jar_cookies(jar: &wreq::cookie::Jar, origin: &str) -> Result<BTreeMap<String, String>> {
    use wreq::cookie::CookieStore;
    let uri: wreq::Uri = origin
        .parse()
        .with_context(|| format!("parsing {origin}"))?;
    match jar.cookies(&uri, wreq::Version::HTTP_11) {
        wreq::cookie::Cookies::Compressed(header) => {
            Ok(parse_cookie_header(header.to_str().unwrap_or_default()))
        }
        wreq::cookie::Cookies::Uncompressed(headers) => {
            let header = headers
                .iter()
                .filter_map(|header| header.to_str().ok())
                .collect::<Vec<_>>()
                .join("; ");
            Ok(parse_cookie_header(&header))
        }
        wreq::cookie::Cookies::Empty => Ok(BTreeMap::new()),
        _ => Ok(BTreeMap::new()),
    }
}

/// `a=1; b=2` into pairs.
pub fn parse_cookie_header(header: &str) -> BTreeMap<String, String> {
    header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// Pull the session out of a Netscape-format `cookies.txt`, which is what
/// browser export extensions and `curl -c` write.
///
/// Lines are tab-separated: domain, subdomains-flag, path, secure, expiry,
/// name, value. Anything from another site is skipped, so a whole-browser
/// export can be handed over as-is.
pub fn cookies_from_netscape(text: &str) -> BTreeMap<String, String> {
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
                .ends_with("woolworths.co.nz")
                .then(|| (fields[5].to_string(), fields[6].to_string()))
        })
        .filter(|(name, _)| {
            name.starts_with(SESSION_COOKIE_PREFIX) || name == crate::session::GUEST_COOKIE
        })
        .collect()
}

/// The value of the login form's hidden `state` field.
///
/// Written by hand rather than with an HTML parser: one attribute on one input,
/// on a page this tool has no other reason to understand.
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
/// and which template version is live, so several markers are tried. The text
/// is whatever sits between that element's tag and the next one.
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

/// The value of one attribute in a tag.
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
    fn an_empty_state_is_skipped_for_a_real_one() {
        let html = r#"<input name="state" value=""><input name="state" value="real">"#;
        assert_eq!(form_state(html).as_deref(), Some("real"));
    }

    #[test]
    fn a_page_with_no_form_has_no_state() {
        assert_eq!(form_state("<html><body>Access denied</body></html>"), None);
    }

    #[test]
    fn entities_in_a_state_are_decoded() {
        let html = r#"<input name="state" value="a&amp;b">"#;
        assert_eq!(form_state(html).as_deref(), Some("a&b"));
    }

    #[test]
    fn an_error_banner_is_read_back_as_the_reason() {
        let html = r#"<span id="error-element-password" class="ulp-input-error-message">
Wrong email or password</span>"#;
        assert_eq!(
            error_message(html).as_deref(),
            Some("Wrong email or password")
        );
        assert_eq!(error_message("<html></html>"), None);
    }

    #[test]
    fn cookie_headers_split_into_pairs() {
        let got = parse_cookie_header("__session__0=a; __session__1=b; other=c");
        assert_eq!(got.get("__session__0").map(String::as_str), Some("a"));
        assert_eq!(got.len(), 3);
        assert!(parse_cookie_header("").is_empty());
    }

    #[test]
    fn a_netscape_export_yields_only_the_woolworths_session() {
        let text = "# Netscape HTTP Cookie File\n\
             www.woolworths.co.nz\tFALSE\t/\tTRUE\t1788386095\t__session__0\tabc\n\
             #HttpOnly_www.woolworths.co.nz\tFALSE\t/\tTRUE\t1788386095\t__session__1\tdef\n\
             www.woolworths.co.nz\tFALSE\t/\tTRUE\t0\t__guest__token\tghi\n\
             .woolworths.co.nz\tTRUE\t/\tTRUE\t1788386095\tbm_sv\tnoise\n\
             www.newworld.co.nz\tFALSE\t/\tTRUE\t1788386095\t__session__0\tnotours\n";
        let got = cookies_from_netscape(text);
        assert_eq!(got.get("__session__0").map(String::as_str), Some("abc"));
        assert_eq!(got.get("__session__1").map(String::as_str), Some("def"));
        assert_eq!(got.get("__guest__token").map(String::as_str), Some("ghi"));
        // Bot-management cookies are not credentials and are not kept.
        assert!(!got.contains_key("bm_sv"));
        assert_eq!(got.len(), 3, "another site's cookies must not leak in");
    }

    #[test]
    fn an_export_with_nothing_relevant_yields_nothing() {
        assert!(cookies_from_netscape("# just a comment\n\n").is_empty());
        assert!(cookies_from_netscape("malformed line without tabs").is_empty());
    }

    #[test]
    fn origins_are_taken_off_the_url() {
        assert_eq!(
            origin_of("https://auth.woolworths.co.nz/u/login/password?state=x"),
            "https://auth.woolworths.co.nz"
        );
    }
}
