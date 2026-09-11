//! What is worth keeping between runs, and where it is filed.
//!
//! Two credentials, held apart because they are earned differently and expire
//! differently:
//!
//! | | where it comes from | how long it lasts | captcha |
//! |---|---|---|---|
//! | Gigya `login_token` | one browser-assisted sign-in | a Gigya session policy | **yes** |
//! | Magento customer token | minted from the above | `customer_token_timeout`, 120 minutes | no |
//!
//! Only the first one is precious. The second is derived, cheap, and refreshed
//! without anyone's involvement -- which is the whole reason this tool stores a
//! `login_token` and never a password.
//!
//! **Per fascia, always.** Briscoes and Rebel Sport are separate SAP Customer
//! Data Cloud sites with separate API keys, so a Briscoes token is not a Rebel
//! Sport one and there is no SSO between them. The state directory is scoped by
//! banner for the same reason `net_kit::Paths::scoped` exists.

use std::time::Duration;

use net_kit::{jwt, Secrets};
use serde::{Deserialize, Serialize};

use crate::banner::Banner;
use crate::error::Result;

/// Renew rather than send a token this close to expiry. A token that expires
/// during the request it authorises fails in a way that reads as bad
/// credentials.
pub const SKEW: Duration = Duration::from_secs(60);

/// What `storeConfig` says, in case a token will not parse: `customer_token_timeout`
/// is 120 minutes on both fascias.
const ASSUMED_LIFETIME: Duration = Duration::from_secs(110 * 60);

/// The credentials one fascia's commands run on.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Session {
    /// Gigya's session token. The thing worth keeping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_token: Option<String>,
    /// The Magento bearer, a JWT.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// When the bearer expires, in milliseconds. Read off the JWT where it
    /// parses, assumed where it does not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// Whose account this is, for `auth status` to have something to show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Gigya's UID -- the numeric SAP customer id on these sites. Cached so a
    /// refresh has something to sign even before `getAccountInfo` answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
}

impl Session {
    /// Whether the Magento bearer is good for the next request.
    pub fn token_fresh(&self) -> bool {
        match (&self.token, self.expires_at) {
            (Some(_), Some(at)) => jwt::fresh(at, SKEW),
            // A token with no readable expiry is worth one attempt: the call
            // that fails is cheaper than a sign-in that was not needed.
            (Some(_), None) => true,
            _ => false,
        }
    }

    /// Whether there is anything to refresh *from*.
    pub fn can_refresh(&self) -> bool {
        self.login_token.is_some()
    }

    pub fn bearer(&self) -> Option<String> {
        Some(format!("Bearer {}", self.token.as_ref()?))
    }

    /// File a newly minted Magento token, reading its expiry off the JWT.
    pub fn set_token(&mut self, token: String) {
        self.expires_at = jwt::expiry_ms(&token)
            .or_else(|| Some(jwt::now_ms() + ASSUMED_LIFETIME.as_millis() as u64));
        self.token = Some(token);
    }

    /// Forget the bearer but keep the Gigya login, which is what a `401`
    /// should cost: the next command mints a new one silently.
    pub fn clear_token(&mut self) {
        self.token = None;
        self.expires_at = None;
    }
}

/// A session as it is filed, and the account it is filed under.
///
/// The account name carries the banner, so one credential store holds both
/// fascias without either being able to read the other by accident.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredSession {
    pub banner: Banner,
    #[serde(flatten)]
    pub session: Session,
}

impl StoredSession {
    fn account(banner: Banner) -> String {
        format!("session.{}", banner.id())
    }

    pub fn load(secrets: &Secrets, banner: Banner) -> Result<Option<StoredSession>> {
        let Some(raw) = secrets.get(&Self::account(banner))? else {
            return Ok(None);
        };
        // A credential store holding something this version cannot read is not
        // worth failing over: the fix is to sign in again, and saying "not
        // signed in" leads there.
        Ok(serde_json::from_str(&raw).ok())
    }

    pub fn save(secrets: &Secrets, banner: Banner, session: &Session) -> Result<()> {
        let stored = StoredSession {
            banner,
            session: session.clone(),
        };
        let raw = serde_json::to_string(&stored)
            .map_err(|e| net_kit::Error::decode("filing the session", e))?;
        secrets.set(&Self::account(banner), &raw)?;
        Ok(())
    }

    pub fn delete(secrets: &Secrets, banner: Banner) -> Result<bool> {
        Ok(secrets.delete(&Self::account(banner))?)
    }

    pub fn session(&self) -> Session {
        self.session.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &std::path::Path) -> Secrets {
        Secrets::new("bgnz-test", net_kit::Backend::File, dir)
    }

    #[test]
    fn a_session_with_no_bearer_is_not_fresh_but_may_still_refresh() {
        // The distinction the whole auth module turns on: having no Magento
        // token is ordinary and silent, having no Gigya login is not.
        let mut s = Session {
            login_token: Some("st2.s.NotARealToken".into()),
            ..Default::default()
        };
        assert!(!s.token_fresh());
        assert!(s.can_refresh());

        s.set_token("not.a.jwt".into());
        assert!(s.token_fresh(), "an unreadable expiry is worth one attempt");
        assert!(s.expires_at.is_some(), "and gets an assumed one");
    }

    #[test]
    fn an_expired_bearer_is_not_fresh() {
        let mut s = Session::default();
        s.set_token("x".into());
        s.expires_at = Some(jwt::now_ms().saturating_sub(1));
        assert!(!s.token_fresh());
    }

    #[test]
    fn clearing_the_bearer_keeps_the_login_that_can_mint_another() {
        let mut s = Session {
            login_token: Some("st2.s.NotARealToken".into()),
            ..Default::default()
        };
        s.set_token("x".into());
        s.clear_token();
        assert!(s.token.is_none());
        assert!(s.can_refresh(), "a 401 must not cost the sign-in");
    }

    #[test]
    fn the_two_fascias_are_filed_apart() {
        // There is no SSO between them, so one store holding both must never
        // let a Briscoes command read a Rebel Sport token.
        let dir = tempfile::tempdir().expect("a temp dir");
        let secrets = store(dir.path());

        let briscoes = Session {
            login_token: Some("st2.s.Briscoes".into()),
            email: Some("shopper@example.invalid".into()),
            ..Default::default()
        };
        StoredSession::save(&secrets, Banner::Briscoes, &briscoes).expect("saves");

        assert!(StoredSession::load(&secrets, Banner::RebelSport)
            .expect("loads")
            .is_none());
        let back = StoredSession::load(&secrets, Banner::Briscoes)
            .expect("loads")
            .expect("was saved");
        assert_eq!(back.banner, Banner::Briscoes);
        assert_eq!(back.session.login_token.as_deref(), Some("st2.s.Briscoes"));

        assert!(StoredSession::delete(&secrets, Banner::Briscoes).expect("deletes"));
        assert!(StoredSession::load(&secrets, Banner::Briscoes)
            .expect("loads")
            .is_none());
    }

    #[test]
    fn an_unreadable_stored_session_reads_as_absent_rather_than_failing() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let secrets = store(dir.path());
        secrets
            .set(&StoredSession::account(Banner::Briscoes), "{not json")
            .expect("writes");
        assert!(StoredSession::load(&secrets, Banner::Briscoes)
            .expect("does not fail")
            .is_none());
    }
}
