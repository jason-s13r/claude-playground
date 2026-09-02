//! The HTTP client.
//!
//! Cloudflare scores the handshake and HTTP/2 settings, not the headers. Every
//! `reqwest` TLS backend is scored as a bot; `wreq` presents a browser
//! fingerprint. Do not add `http1_only()`: a browser fingerprint speaking
//! HTTP/1.1 is itself inconsistent and gets challenged.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use wreq_util::Emulation;

use crate::cookies;

/// The browser emulated; handshake, HTTP/2 settings and headers all derive from
/// it. The OS rides along, macOS on every host, so one build is one device.
/// Club Plus scores device identity, so changing this costs one verification.
pub const EMULATION: Emulation = Emulation::Chrome137;

pub fn client(jar: Arc<cookies::Jar>) -> Result<wreq::Client> {
    Ok(wreq::Client::builder()
        .emulation(EMULATION)
        // Cloudflare's `__cf_bm` marks a session it has already scored.
        .cookie_provider(jar)
        // wreq follows none by default, unlike reqwest.
        .redirect(wreq::redirect::Policy::limited(10))
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()?)
}

/// The `User-Agent` the emulation sends. The SSO exchange echoes it in the
/// request body as `fingerprintGuest`, the one place it must be named.
pub fn user_agent(http: &wreq::Client) -> String {
    http.headers()
        .get(wreq::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}
