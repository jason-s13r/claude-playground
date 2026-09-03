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
//! browser handshake [`crate::EMULATION`] selects, the same requests are
//! answered normally.
//!
//! It is still somebody else's login page, and a change to it will break this.
//! A session can also be filled in from cookies exported from a browser, which
//! is the way back in when that happens.

use std::collections::BTreeMap;
use std::sync::Arc;

use net_kit::wreq;

use crate::endpoints::Endpoints;
use crate::error::{Error, Result};
use crate::http::EMULATION;
use crate::session::{Session, GUEST_COOKIE, SESSION_COOKIE_PREFIX};

/// Narrates each step. The caller supplies it, rather than this crate reading a
/// debug environment variable.
///
/// Nothing passed to it is a secret: query strings are dropped (they carry the
/// flow's `state`), cookies appear by name only, and no credential is ever
/// formatted.
pub type Trace<'a> = &'a dyn Fn(&str, &str);

/// A trace that discards everything.
pub fn no_trace(_step: &str, _detail: &str) {}

/// Follow the login flow and return the cookies it ends with.
pub async fn login(
    endpoints: &Endpoints,
    email: &str,
    password: &str,
    trace: Trace<'_>,
) -> Result<Session> {
    let jar = Arc::new(wreq::cookie::Jar::default());
    // Its own client, not the shared one: this is the one place redirects must
    // be followed. Twenty rather than ten because a login that loops is better
    // reported as a loop than as a mystery.
    let http = net_kit::http::build(
        net_kit::ClientSpec::new(EMULATION, wreq::redirect::Policy::limited(20))
            .with_cookies(jar.clone()),
    )
    .map_err(|source| net_kit::HttpError::Transport {
        method: "BUILD",
        url: endpoints.auth.clone(),
        source,
    })?;

    // Step one: the storefront hands the whole OAuth request to Auth0, which
    // answers with the email form.
    let start = format!("{}/auth/login?returnTo=/", endpoints.origin);
    let page = get(&http, "start", &start, trace).await?;
    let state = form_state(&page.html).ok_or_else(|| Error::NoSession {
        detail: format!(
            ": the sign-in page at {start} carried no login form. Woolworths may have \
             changed it, or be serving a bot check; importing cookies from a browser \
             is the way past that."
        ),
    })?;

    // Step two: the email. Auth0 splits identifier and password across two
    // pages and rejects the password form if the identifier one was skipped.
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
        trace,
    )
    .await?;

    // A 4xx landing back on the identifier form is Auth0 refusing the email.
    // The re-rendered form carries a `state` of its own, so scraping one is not
    // evidence the step succeeded and this has to be checked first.
    if page.bounced(&format!("{}/u/login/identifier", endpoints.auth)) {
        return Err(Error::LoginRefused {
            step: "email address",
            detail: reason(&page.html),
        });
    }

    // The password page carries a fresh state; reusing the first one fails.
    let state = form_state(&page.html).ok_or_else(|| Error::LoginRefused {
        step: "email address",
        detail: reason(&page.html),
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
        trace,
    )
    .await?;

    if page.bounced(&format!("{}/u/login/password", endpoints.auth)) {
        return Err(Error::LoginRefused {
            step: "password",
            detail: reason(&page.html),
        });
    }

    let cookies = jar_cookies(&jar, &endpoints.origin);
    trace(
        "password",
        &format!("cookies on the storefront: [{}]", names(&cookies)),
    );
    let session = Session::from_cookies(cookies);
    if !session.account {
        // Landing back on a form means Auth0 rejected something rather than
        // completing the flow, and the page usually says what.
        return Err(Error::NoSession {
            detail: reason(&page.html),
        });
    }
    Ok(session)
}

/// Pull a session out of a Netscape-format `cookies.txt`, which is what browser
/// export extensions and `curl -c` write.
///
/// The way back in when the sign-in page changes shape.
pub fn from_netscape(text: &str) -> Session {
    let cookies: BTreeMap<String, String> =
        net_kit::cookies::from_netscape(text, "woolworths.co.nz")
            .into_iter()
            .filter(|(name, _)| name.starts_with(SESSION_COOKIE_PREFIX) || name == GUEST_COOKIE)
            .collect();
    Session::from_cookies(cookies)
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
    // fact when the flow goes somewhere unexpected.
    let landed = res.uri().to_string();
    let landed_path = url::Url::parse(&landed)
        .map(|u| format!("{}{}", u.origin().ascii_serialization(), u.path()))
        .unwrap_or_else(|_| landed.clone());
    let html = res.text().await.unwrap_or_default();
    trace(
        step,
        &format!(
            "{status} -> {landed_path} ({} bytes){}",
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
fn jar_cookies(jar: &wreq::cookie::Jar, origin: &str) -> BTreeMap<String, String> {
    use wreq::cookie::CookieStore;
    let Ok(uri) = origin.parse::<wreq::Uri>() else {
        return BTreeMap::new();
    };
    match jar.cookies(&uri, wreq::Version::HTTP_11) {
        wreq::cookie::Cookies::Compressed(header) => {
            net_kit::cookies::parse_cookie_header(header.to_str().unwrap_or_default())
        }
        wreq::cookie::Cookies::Uncompressed(headers) => {
            let header = headers
                .iter()
                .filter_map(|h| h.to_str().ok())
                .collect::<Vec<_>>()
                .join("; ");
            net_kit::cookies::parse_cookie_header(&header)
        }
        _ => BTreeMap::new(),
    }
}

/// The value of the login form's hidden `state` field.
///
/// Written by hand rather than with an HTML parser: one attribute on one input,
/// on a page this crate has no other reason to understand.
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
    fn a_page_with_no_form_has_no_state() {
        assert!(form_state("<html><body>Just a moment...</body></html>").is_none());
    }

    #[test]
    fn entities_in_the_state_are_decoded() {
        let html = r#"<input name="state" value="a&amp;b">"#;
        assert_eq!(form_state(html).as_deref(), Some("a&b"));
    }

    #[test]
    fn an_auth0_banner_is_read_however_it_is_rendered() {
        for html in [
            r#"<span id="error-element-password">Wrong email or password</span>"#,
            r#"<div class="ulp-input-error-message">Wrong email or password</div>"#,
            r#"<div role="alert">Wrong email or password</div>"#,
        ] {
            assert_eq!(
                error_message(html).as_deref(),
                Some("Wrong email or password"),
                "{html}"
            );
        }
        assert_eq!(error_message("<p>nothing wrong</p>"), None);
    }

    #[test]
    fn a_bounce_is_the_same_path_coming_back_with_a_client_error() {
        let page = Page {
            landed: "https://auth.example.test/u/login/password?state=x".into(),
            rejected: true,
            html: String::new(),
        };
        assert!(page.bounced("https://auth.example.test/u/login/password"));
        assert!(!page.bounced("https://auth.example.test/u/login/identifier"));

        let moved = Page {
            landed: "https://www.example.test/".into(),
            rejected: false,
            html: String::new(),
        };
        assert!(!moved.bounced("https://auth.example.test/u/login/password"));
    }

    #[test]
    fn an_imported_jar_keeps_only_the_session_and_guest_cookies() {
        let text = "\
.woolworths.co.nz\tTRUE\t/\tTRUE\t1900000000\t__session__0\tpart-a
.woolworths.co.nz\tTRUE\t/\tTRUE\t1900000000\t__session__1\tpart-b
.woolworths.co.nz\tTRUE\t/\tTRUE\t1900000000\tak_bmsc\tbotmanager
.other-site.test\tTRUE\t/\tTRUE\t1900000000\t__session__0\tnot-ours
";
        let session = from_netscape(text);
        assert!(session.account);
        assert_eq!(
            session.header().as_deref(),
            Some("__session__0=part-a; __session__1=part-b"),
            "another site's cookies and the bot-manager cookie are left out"
        );
    }

    #[test]
    fn an_import_with_nothing_useful_is_not_an_account() {
        assert!(!from_netscape("# empty\n").account);
    }
}
