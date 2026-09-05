//! What a request is made with.
//!
//! Kmart needs **two independent credentials**, and the whole shape of this
//! module follows from their being independent:
//!
//! - a **bearer token** from Auth0, which says who you are. The CLI can obtain
//!   this itself: the login flow is not bot-protected, so an email and a
//!   password are enough, and `offline_access` means the refresh token renews
//!   it thereafter without asking again.
//! - **Akamai cookies** for the gateway, which say you are a browser. The CLI
//!   cannot obtain these -- passing the check means running Akamai's sensor
//!   script -- so they are imported from a browser export.
//!
//! Either can be present without the other, and the failures read completely
//! differently: no token is "sign in", no cookies is "import cookies", and a
//! client that conflated them would tell people to do the wrong one. So
//! [`Session::signed_in`] and [`Session::admitted`] are separate questions and
//! both are asked.
//!
//! Nothing here is needed for the catalogue, which is why search and browse
//! work on a machine that has never seen either.

use std::collections::BTreeMap;

use net_kit::{wreq, Secrets};
use serde::{Deserialize, Serialize};

use crate::country::Country;
use crate::error::{Error, Result};

/// Where a stored session is filed in the credential store.
pub const ACCOUNT: &str = "session";

/// Renew this long before the token expires, so one does not lapse midway
/// through a command that makes several calls. Tokens live 15 minutes, so this
/// is a fifth of the life rather than a rounding error.
const EXPIRY_MARGIN_SECS: u64 = 180;

/// The cookies worth keeping out of a browser export.
///
/// Akamai's, and nothing else. An export is a whole browser profile -- ad
/// ids, analytics, session replay -- and none of it belongs in a credential
/// store. `_abck` is the one that actually matters; the `bm_*` family and
/// `ak_bmsc` travel with it and the check fails without them.
pub fn is_admission_cookie(name: &str) -> bool {
    name == "_abck" || name == "ak_bmsc" || name.starts_with("bm_")
}

/// An Auth0 token pair.
#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access: String,
    /// Present because the storefront asks for `offline_access`. Without it a
    /// lapsed session would need the password again every fifteen minutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<String>,
    /// Unix seconds. Taken from the JWT's own `exp` where it parses, and from
    /// `expires_in` where it does not.
    pub expires_at: u64,
}

/// Names and times only. The values are credentials.
impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens")
            .field("access", &"<redacted>")
            .field("refresh", &self.refresh.as_ref().map(|_| "<redacted>"))
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl Tokens {
    /// Build from what `/oauth/token` answered.
    pub fn new(access: String, refresh: Option<String>, expires_in: Option<u64>) -> Tokens {
        // The JWT's own claim is preferred: it is what the gateway will
        // actually enforce, and `expires_in` is relative to a clock that may
        // not be this one.
        let expires_at = net_kit::jwt::expiry_ms(&access)
            .map(|ms| ms / 1000)
            .unwrap_or_else(|| net_kit::jwt::now_secs() + expires_in.unwrap_or(900));
        Tokens {
            access,
            refresh,
            expires_at,
        }
    }

    /// A pair holding only a refresh token.
    ///
    /// The shape a session takes when the token was lifted out of a browser
    /// rather than obtained by signing in: there is nothing to authorise a
    /// request with yet, and the first call renews. Deliberately born lapsed,
    /// so that is exactly what happens.
    pub fn from_refresh(refresh: impl Into<String>) -> Tokens {
        Tokens {
            access: String::new(),
            refresh: Some(refresh.into()),
            expires_at: 0,
        }
    }

    /// Whether this has run out, or is about to.
    pub fn lapsed(&self) -> bool {
        self.expires_at <= net_kit::jwt::now_secs() + EXPIRY_MARGIN_SECS
    }

    /// Whose it is, off the token's own claims. Unverified -- this reads what
    /// Auth0 said rather than checking it.
    pub fn subject(&self) -> Option<String> {
        net_kit::jwt::claim_str(&self.access, "sub")
    }
}

/// One country's admission cookies.
pub type Cookies = BTreeMap<String, String>;

/// The credentials one request is made with.
#[derive(Clone, Default)]
pub struct Session {
    tokens: Option<Tokens>,
    /// Per country, because the two storefronts are different Akamai origins
    /// and a cookie earned on one is not accepted by the other.
    cookies: BTreeMap<Country, Cookies>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("tokens", &self.tokens)
            .field(
                "cookies",
                &self
                    .cookies
                    .iter()
                    .map(|(c, v)| (c.code(), v.keys().collect::<Vec<_>>()))
                    .collect::<BTreeMap<_, _>>(),
            )
            .finish()
    }
}

impl Session {
    pub fn new() -> Session {
        Session::default()
    }

    pub fn with_tokens(mut self, tokens: Tokens) -> Session {
        self.tokens = Some(tokens);
        self
    }

    pub fn with_cookies(mut self, country: Country, cookies: Cookies) -> Session {
        self.cookies.insert(country, cookies);
        self
    }

    pub fn tokens(&self) -> Option<&Tokens> {
        self.tokens.as_ref()
    }

    /// Whether there is an account behind this.
    pub fn signed_in(&self) -> bool {
        self.tokens.is_some()
    }

    /// Whether the gateway will look at a request at all.
    ///
    /// A different question from [`Session::signed_in`], and the more common
    /// failure: cookies expire in about a day, tokens renew themselves.
    pub fn admitted(&self, country: Country) -> bool {
        self.cookies
            .get(&country)
            .is_some_and(|c| c.contains_key("_abck"))
    }

    /// One country's admission cookies, or an empty set when there are none.
    ///
    /// The login flow uses this to seed its jar: the auth host is
    /// `auth.kmart.com.au`, whose `_abck` is scoped to `.kmart.com.au`, so the
    /// Australian bucket is what covers it whichever country is signing in.
    pub fn admission(&self, country: Country) -> Cookies {
        self.cookies.get(&country).cloned().unwrap_or_default()
    }

    /// The `Cookie` header for one country, or `None` when there is nothing to
    /// send.
    pub fn cookie_header(&self, country: Country) -> Option<String> {
        let cookies = self.cookies.get(&country)?;
        if cookies.is_empty() {
            return None;
        }
        Some(
            cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    /// The `Authorization` header, when there is a usable token.
    ///
    /// `None` for an access token that is empty, which is what a session
    /// imported as a bare refresh token holds until it has renewed once.
    pub fn bearer(&self) -> Option<String> {
        let t = self.tokens.as_ref()?;
        (!t.access.is_empty()).then(|| format!("Bearer {}", t.access))
    }

    /// Fold in what a response set, so cookies earned mid-run are kept.
    pub fn absorb(&mut self, country: Country, headers: &wreq::header::HeaderMap) {
        let jar = self.cookies.entry(country).or_default();
        for (name, value) in net_kit::cookies::set_cookies(headers) {
            if !is_admission_cookie(&name) {
                continue;
            }
            if value.is_empty() {
                jar.remove(&name);
            } else {
                jar.insert(name, value);
            }
        }
    }
}

/// A session on disk.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct StoredSession {
    /// The email it was obtained for, so a status command can name it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Tokens>,
    /// Which storefront signed this in, by country code.
    ///
    /// Not a preference -- the two countries are separate Auth0 applications,
    /// and a refresh token is bound to the one that minted it. Renewing needs
    /// the client id that issued it, whatever country the command is asking
    /// about. Absent in a session written before this was known, in which case
    /// the command's own country is the only guess available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_country: Option<String>,
    /// Keyed by country code, because the store is JSON and a map key has to
    /// be a string.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cookies: BTreeMap<String, Cookies>,
    #[serde(default)]
    pub obtained_at: u64,
}

impl std::fmt::Debug for StoredSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredSession")
            .field("email", &self.email)
            .field("tokens", &self.tokens)
            .field(
                "cookies",
                &self
                    .cookies
                    .iter()
                    .map(|(c, v)| (c, v.keys().collect::<Vec<_>>()))
                    .collect::<BTreeMap<_, _>>(),
            )
            .finish()
    }
}

impl StoredSession {
    pub fn of(session: &Session, email: Option<String>) -> StoredSession {
        StoredSession {
            email,
            auth_country: None,
            tokens: session.tokens.clone(),
            cookies: session
                .cookies
                .iter()
                .map(|(c, v)| (c.code().to_string(), v.clone()))
                .collect(),
            obtained_at: net_kit::jwt::now_secs(),
        }
    }

    /// Say which storefront signed this in.
    ///
    /// Separate from [`StoredSession::of`] because that rebuilds the record
    /// from a live session, which knows the tokens but not where they were
    /// obtained -- and a field that is silently dropped on every renewal is
    /// worse than one that was never stored.
    pub fn with_auth_country(mut self, country: Option<Country>) -> StoredSession {
        self.auth_country = country.map(|c| c.code().to_string());
        self
    }

    /// Which storefront signed this in, if it was recorded.
    pub fn auth_country(&self) -> Option<Country> {
        self.auth_country.as_deref().and_then(Country::parse)
    }

    pub fn load(secrets: &Secrets) -> Result<Option<StoredSession>> {
        let Some(text) = secrets.get(ACCOUNT)? else {
            return Ok(None);
        };
        // A session written by an older build, or corrupted, is worth
        // discarding rather than failing every command until it is removed by
        // hand.
        Ok(serde_json::from_str(&text).ok())
    }

    pub fn save(&self, secrets: &Secrets) -> Result<()> {
        let text =
            serde_json::to_string(self).map_err(|e| Error::decode("serialising the session", e))?;
        Ok(secrets.set(ACCOUNT, &text)?)
    }

    pub fn clear(secrets: &Secrets) -> Result<bool> {
        Ok(secrets.delete(ACCOUNT)?)
    }

    pub fn session(&self) -> Session {
        let mut session = Session::new();
        if let Some(tokens) = self.tokens.clone() {
            session = session.with_tokens(tokens);
        }
        for (code, cookies) in &self.cookies {
            if let Some(country) = Country::parse(code) {
                session = session.with_cookies(country, cookies.clone());
            }
        }
        session
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net_kit::Backend;

    fn jwt(exp: u64, sub: &str) -> String {
        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::json!({ "exp": exp, "sub": sub }).to_string());
        format!("header.{payload}.signature")
    }

    fn cookies(pairs: &[(&str, &str)]) -> Cookies {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn being_signed_in_and_being_let_in_are_separate_questions() {
        // The two failures need opposite advice, so nothing may collapse them.
        let token_only = Session::new().with_tokens(Tokens::new(
            jwt(net_kit::jwt::now_secs() + 900, "auth0|abc"),
            None,
            None,
        ));
        assert!(token_only.signed_in());
        assert!(!token_only.admitted(Country::Nz));

        let cookies_only = Session::new().with_cookies(Country::Nz, cookies(&[("_abck", "valid")]));
        assert!(!cookies_only.signed_in());
        assert!(cookies_only.admitted(Country::Nz));
    }

    #[test]
    fn admission_does_not_carry_across_the_tasman() {
        // Two Akamai origins; a cookie earned on one is refused by the other.
        let s = Session::new().with_cookies(Country::Nz, cookies(&[("_abck", "v")]));
        assert!(s.admitted(Country::Nz));
        assert!(!s.admitted(Country::Au));
        assert!(s.cookie_header(Country::Au).is_none());
    }

    #[test]
    fn admission_needs_the_cookie_that_actually_carries_the_verdict() {
        // The bm_* family travels with `_abck` but says nothing on its own, so
        // a jar holding only those is not admission.
        let s = Session::new().with_cookies(Country::Nz, cookies(&[("bm_sz", "v")]));
        assert!(!s.admitted(Country::Nz));
    }

    #[test]
    fn only_the_bot_manager_cookies_are_kept_from_an_export() {
        for name in ["_abck", "ak_bmsc", "bm_sz", "bm_s", "bm_so"] {
            assert!(is_admission_cookie(name), "{name}");
        }
        for name in ["utag_main_v_id", "optimizelyEndUserId", "ko_token", "_fbp"] {
            assert!(!is_admission_cookie(name), "{name}");
        }
    }

    #[test]
    fn a_token_takes_its_expiry_from_its_own_claim_not_the_clock() {
        let exp = net_kit::jwt::now_secs() + 4242;
        let t = Tokens::new(jwt(exp, "auth0|abc"), None, Some(900));
        assert_eq!(t.expires_at, exp, "the claim wins over expires_in");
        assert_eq!(t.subject().as_deref(), Some("auth0|abc"));
        assert!(!t.lapsed());
    }

    #[test]
    fn an_opaque_token_falls_back_to_expires_in() {
        let t = Tokens::new("not-a-jwt".into(), None, Some(900));
        assert!(t.expires_at > net_kit::jwt::now_secs() + 800);
        assert_eq!(t.subject(), None);
    }

    #[test]
    fn a_bare_refresh_token_is_a_session_that_has_not_renewed_yet() {
        let session = Session::new().with_tokens(Tokens::from_refresh("r"));
        assert!(session.signed_in(), "there is something to renew from");
        assert!(session.bearer().is_none(), "but nothing to send yet");
        assert!(
            session.tokens().unwrap().lapsed(),
            "so the first call renews"
        );
    }

    #[test]
    fn a_token_inside_the_margin_is_already_lapsed() {
        // Tokens live fifteen minutes; one expiring in sixty seconds would
        // die midway through a command that makes several calls.
        let t = Tokens::new(jwt(net_kit::jwt::now_secs() + 60, "s"), None, None);
        assert!(t.lapsed());
    }

    #[test]
    fn credentials_are_never_printed() {
        let s = Session::new()
            .with_tokens(Tokens::new(
                jwt(net_kit::jwt::now_secs() + 900, "auth0|abc"),
                Some("refresh-secret".into()),
                None,
            ))
            .with_cookies(Country::Nz, cookies(&[("_abck", "cookie-secret")]));
        let text = format!("{s:?}");
        assert!(text.contains("_abck"), "names are fine: {text}");
        assert!(!text.contains("cookie-secret"), "{text}");
        assert!(!text.contains("refresh-secret"), "{text}");
    }

    #[test]
    fn a_stored_session_round_trips_and_a_corrupt_one_reads_as_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let secrets = Secrets::new("kmart-api-test", Backend::File, dir.path());
        assert!(StoredSession::load(&secrets).unwrap().is_none());

        let session = Session::new()
            .with_tokens(Tokens::new(
                jwt(net_kit::jwt::now_secs() + 900, "auth0|abc"),
                Some("r".into()),
                None,
            ))
            .with_cookies(Country::Au, cookies(&[("_abck", "v")]));
        StoredSession::of(&session, Some("shopper@example.test".into()))
            .save(&secrets)
            .unwrap();

        let back = StoredSession::load(&secrets).unwrap().unwrap().session();
        assert!(back.signed_in());
        assert!(back.admitted(Country::Au));
        assert!(!back.admitted(Country::Nz));

        secrets.set(ACCOUNT, "{ truncated").unwrap();
        assert!(StoredSession::load(&secrets).unwrap().is_none());
    }

    #[test]
    fn absorbing_a_response_keeps_the_bot_cookies_and_drops_the_rest() {
        let mut headers = wreq::header::HeaderMap::new();
        headers.append(
            wreq::header::SET_COOKIE,
            "_abck=fresh; Path=/; Max-Age=3600".parse().unwrap(),
        );
        headers.append(
            wreq::header::SET_COOKIE,
            "utag_main_v_id=tracking; Path=/".parse().unwrap(),
        );
        let mut s = Session::new();
        s.absorb(Country::Nz, &headers);
        assert!(s.admitted(Country::Nz));
        let header = s.cookie_header(Country::Nz).unwrap();
        assert!(header.contains("_abck=fresh"));
        assert!(!header.contains("utag_main"), "{header}");
    }
}
