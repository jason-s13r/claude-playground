//! The Club Plus session as this tool keeps it: the device identity the login
//! API insists on, the credential blob on disk, and renewal on demand.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::auth::clubplus::{refresh, Session};
use crate::auth::jwt;
use crate::config::Paths;
use crate::secrets::Secrets;

/// A stable per-installation device identifier.
///
/// `POST /user/login` rejects a request without `x-device-id`. The browser
/// generates one and keeps it, so this does the same rather than presenting a
/// brand new device on every login.
pub fn device_id(paths: &Paths) -> Result<String> {
    let file = paths.state_dir.join("device-id");
    if let Ok(existing) = std::fs::read_to_string(&file) {
        let existing = existing.trim().to_string();
        if !existing.is_empty() {
            return Ok(existing);
        }
    }
    let fresh = uuid::Uuid::new_v4().to_string();
    std::fs::create_dir_all(&paths.state_dir)
        .with_context(|| format!("creating {}", paths.state_dir.display()))?;
    std::fs::write(&file, &fresh).with_context(|| format!("writing {}", file.display()))?;
    Ok(fresh)
}
/// One Club Plus account serves both banners, so there is only ever one.
pub const ACCOUNT: &str = "clubplus";

#[derive(Clone, Debug, serde::Serialize, Deserialize)]
pub struct StoredLogin {
    pub email: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// When the session was last minted or renewed. Absent on logins stored by
    /// an older build, which is why it is optional rather than defaulted to
    /// zero -- "unknown" and "the epoch" are different things to report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refreshed_at_ms: Option<u64>,
}

impl StoredLogin {
    pub fn session(&self) -> Session {
        Session {
            access_token: self.access_token.clone(),
            refresh_token: self.refresh_token.clone(),
        }
    }

    /// When the Club Plus access token expires, per its own `exp` claim.
    pub fn expires_at_ms(&self) -> Option<u64> {
        jwt::expiry_ms(&self.access_token)
    }

    /// Usable for a little longer yet. An unparseable token counts as stale, so
    /// the renewal path runs rather than a doomed request.
    pub fn is_fresh(&self) -> bool {
        self.expires_at_ms().is_some_and(crate::token::fresh)
    }

    pub fn can_renew(&self) -> bool {
        self.refresh_token.is_some()
    }
}

/// A Club Plus session known to be usable, and how it was obtained.
pub struct ActiveSession {
    pub session: Session,
    /// True when this call had to renew the session to get here.
    pub renewed: bool,
}

/// The stored session, renewed first if it is at or past expiry.
///
/// This is what keeps a login working past its half-hour: every command that
/// needs an account token comes through here, so the session is renewed on
/// demand rather than only when someone remembers to log in again.
///
/// The rotated refresh token is saved before the session is handed back. If
/// that write were skipped and the process then died, the token just spent
/// would be gone and the only way back would be a password.
pub async fn active_session(
    secrets: &Secrets,
    paths: &Paths,
    force: bool,
) -> Result<ActiveSession> {
    let stored = load(secrets)?.context("not logged in; run `fsnz auth login`")?;

    if !force && stored.is_fresh() {
        return Ok(ActiveSession {
            session: stored.session(),
            renewed: false,
        });
    }

    let Some(refresh_token) = stored.refresh_token.as_deref() else {
        bail!(
            "the stored Club Plus session has expired and carries no refresh \
             token; run `fsnz auth login`"
        );
    };

    let session = refresh(refresh_token, &device_id(paths)?).await?;
    save(
        secrets,
        &StoredLogin {
            email: stored.email,
            access_token: session.access_token.clone(),
            refresh_token: session.refresh_token.clone(),
            refreshed_at_ms: Some(crate::token::now_ms()),
        },
    )?;
    Ok(ActiveSession {
        session,
        renewed: true,
    })
}
pub fn load(secrets: &Secrets) -> Result<Option<StoredLogin>> {
    match secrets.get(ACCOUNT)? {
        // A stored blob we cannot parse is treated as absent: the fix is to log
        // in again, not to make every command fail.
        Some(raw) => Ok(serde_json::from_str(&raw).ok()),
        None => Ok(None),
    }
}

pub fn save(secrets: &Secrets, login: &StoredLogin) -> Result<()> {
    let raw = serde_json::to_string(login).context("serialising the login")?;
    secrets.set(ACCOUNT, &raw)
}

pub fn clear(secrets: &Secrets) -> Result<bool> {
    secrets.delete(ACCOUNT)
}
