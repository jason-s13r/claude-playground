//! The Club Plus password, kept beside the session so a login that has lapsed
//! can be renewed with nobody at the keyboard.
//!
//! A password is a worse thing to hold than a session: it does not expire, and
//! it is the whole account rather than one device's access to it. So it is
//! only written when there is nothing better -- `password_command` wins where
//! it is set, and a manager stays the single copy -- and `fsnz auth logout`
//! removes it along with everything else.

use anyhow::Result;

use crate::secrets::Secrets;

/// Filed apart from the session. `active_session` rewrites the login blob on
/// every renewal, so a password living inside it would be dropped the first
/// time a token was refreshed.
const ACCOUNT: &str = "password";

/// Stored as a JSON string, not raw: the file backend trims what it reads
/// back, which would quietly corrupt a password with leading or trailing
/// space.
pub fn save(secrets: &Secrets, password: &str) -> Result<()> {
    let raw = serde_json::to_string(password).expect("a string always serialises");
    secrets.set(ACCOUNT, &raw)
}

pub fn load(secrets: &Secrets) -> Result<Option<String>> {
    Ok(secrets
        .get(ACCOUNT)?
        .and_then(|raw| serde_json::from_str::<String>(&raw).ok())
        .filter(|p| !p.is_empty()))
}

pub fn clear(secrets: &Secrets) -> Result<bool> {
    secrets.delete(ACCOUNT)
}

/// Where an unattended login gets its password.
#[derive(Clone, Debug)]
pub enum Source {
    /// The configured `password_command`.
    Command(String),
    /// The copy in the credential store.
    Stored(String),
}

impl Source {
    /// The command first: where one is configured it is the account's real
    /// source of truth, and `auth login` keeps no copy alongside it.
    pub fn resolve(command: Option<&str>, secrets: &Secrets) -> Result<Option<Source>> {
        if let Some(cmd) = command.map(str::trim).filter(|c| !c.is_empty()) {
            return Ok(Some(Source::Command(cmd.to_string())));
        }
        Ok(load(secrets)?.map(Source::Stored))
    }

    pub async fn password(&self) -> Result<String> {
        match self {
            Source::Command(cmd) => crate::process::run::capturing(cmd).await,
            Source::Stored(password) => Ok(password.clone()),
        }
    }

    pub fn describe(&self) -> &'static str {
        match self {
            Source::Command(_) => "the configured password_command",
            Source::Stored(_) => "the stored password",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store(dir: &TempDir) -> Secrets {
        std::env::set_var("FSNZ_SECRET_BACKEND", "file");
        Secrets::new(dir.path().to_path_buf())
    }

    #[test]
    fn round_trips_a_password() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        assert_eq!(load(&s).unwrap(), None);
        save(&s, "hunter2").unwrap();
        assert_eq!(load(&s).unwrap().as_deref(), Some("hunter2"));
        assert!(clear(&s).unwrap());
        assert_eq!(load(&s).unwrap(), None);
    }

    #[test]
    fn surrounding_space_survives_the_round_trip() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        save(&s, "  spaced  ").unwrap();
        assert_eq!(load(&s).unwrap().as_deref(), Some("  spaced  "));
    }

    #[test]
    fn a_configured_command_beats_the_stored_copy() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        save(&s, "stored").unwrap();
        assert!(matches!(
            Source::resolve(Some("pass show clubplus"), &s).unwrap(),
            Some(Source::Command(_))
        ));
        assert!(matches!(
            Source::resolve(Some("   "), &s).unwrap(),
            Some(Source::Stored(_))
        ));
    }

    #[test]
    fn nothing_stored_and_no_command_is_no_source() {
        let dir = TempDir::new().unwrap();
        assert!(Source::resolve(None, &store(&dir)).unwrap().is_none());
    }
}
