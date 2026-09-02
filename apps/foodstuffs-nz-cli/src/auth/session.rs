//! The Club Plus session as this tool keeps it: the device identity the login
//! API insists on, the credential blob on disk, and renewal on demand.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::auth::clubplus::{refresh, Session};
use crate::auth::jwt;
use crate::auth::password;
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
/// Renewal is tried from the refresh token first, and -- when that is gone or
/// has been spent -- from a password, which is what makes an unattended run
/// survive an interruption longer than one refresh token. Neither path can
/// answer a verification code, so a device Club Plus wants to challenge still
/// needs `fsnz auth login`.
///
/// The rotated refresh token is saved before the session is handed back. If
/// that write were skipped and the process then died, the token just spent
/// would be gone and the only way back would be a password.
pub async fn active_session(
    http: &wreq::Client,
    secrets: &Secrets,
    paths: &Paths,
    password_command: Option<&str>,
    force: bool,
) -> Result<ActiveSession> {
    let stored = load(secrets)?.context("not logged in; run `fsnz auth login`")?;

    if !force && stored.is_fresh() {
        return Ok(ActiveSession {
            session: stored.session(),
            renewed: false,
        });
    }

    let device_id = device_id(paths)?;
    let source = password::Source::resolve(password_command, secrets)?;

    if let Some(refresh_token) = stored.refresh_token.as_deref() {
        match refresh(http, refresh_token, &device_id).await {
            Ok(session) => {
                persist(secrets, &stored.email, &session)?;
                return Ok(ActiveSession {
                    session,
                    renewed: true,
                });
            }
            // Without a password this is the end of the road, so the reason
            // Club Plus gave is the answer rather than a generic one.
            Err(e) if source.is_none() => return Err(e),
            // A refresh token already spent, or a session ended elsewhere.
            // Signing in again is the only way past it.
            Err(_) => {}
        }
    }

    let Some(source) = source else {
        bail!(
            "the stored Club Plus session has expired and carries no refresh \
             token; run `fsnz auth login`"
        );
    };
    let session = sign_in(http, &stored.email, &source, &device_id).await?;
    persist(secrets, &stored.email, &session)?;
    Ok(ActiveSession {
        session,
        renewed: true,
    })
}

/// A whole new login, for when renewal is no longer possible.
///
/// Fails rather than prompts on a verification code: this runs underneath
/// ordinary commands, where there is not necessarily anyone to type one.
async fn sign_in(
    http: &wreq::Client,
    email: &str,
    source: &password::Source,
    device_id: &str,
) -> Result<Session> {
    let password = source.password().await?;
    match crate::auth::login(http, email, &password, device_id).await? {
        crate::auth::Login::Complete(session) => Ok(session),
        crate::auth::Login::ChallengeRequired(challenge) => bail!(
            "the Club Plus session could not be renewed, and signing in again \
             with {} needs a verification code ({}) sent to {email}. Run \
             `fsnz auth login`.",
            source.describe(),
            challenge.method
        ),
    }
}

fn persist(secrets: &Secrets, email: &str, session: &Session) -> Result<()> {
    save(
        secrets,
        &StoredLogin {
            email: email.to_string(),
            access_token: session.access_token.clone(),
            refresh_token: session.refresh_token.clone(),
            refreshed_at_ms: Some(crate::token::now_ms()),
        },
    )
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
