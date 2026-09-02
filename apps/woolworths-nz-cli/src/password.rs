//! The account password, kept beside the session so a sign-in that has lapsed
//! can be renewed with nobody at the keyboard.
//!
//! A Woolworths session carries no readable expiry and nothing to refresh it
//! with -- the cookie is encrypted and only the site can mint one -- so walking
//! the login flow again is the only renewal there is, and a password is what
//! that takes. It is a heavier thing to hold than the session it renews, so
//! `wwnz auth logout` removes it along with everything else.

use anyhow::{bail, Context, Result};

use crate::secrets::Secrets;

/// Filed apart from the session, which `import` and every re-login rewrite.
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

/// Where an unattended sign-in gets its password.
#[derive(Clone, Debug)]
pub enum Source {
    /// The configured `password_command`.
    Command(String),
    /// The copy in the credential store.
    Stored(String),
}

impl Source {
    /// The command first: where one is configured it is the account's real
    /// source of truth, and a password changed there should take effect
    /// without another `auth login`.
    pub fn resolve(command: Option<&str>, secrets: &Secrets) -> Result<Option<Source>> {
        if let Some(cmd) = command.map(str::trim).filter(|c| !c.is_empty()) {
            return Ok(Some(Source::Command(cmd.to_string())));
        }
        Ok(load(secrets)?.map(Source::Stored))
    }

    pub fn password(&self) -> Result<String> {
        match self {
            Source::Command(cmd) => from_command(cmd),
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

/// Run a command and take its first line of stdout as the password.
///
/// The point is a password manager: `password_command = "pass show woolworths"`
/// keeps the password out of the config file and out of the shell history.
pub fn from_command(command: &str) -> Result<String> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .with_context(|| format!("running the password command: {command}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "the password command failed ({}): {}",
            output.status,
            stderr.trim()
        );
    }
    let password = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if password.is_empty() {
        bail!("the password command printed nothing: {command}");
    }
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store(dir: &TempDir) -> Secrets {
        std::env::set_var("WWNZ_SECRET_BACKEND", "file");
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
            Source::resolve(Some("pass show woolworths"), &s).unwrap(),
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

    #[test]
    fn the_password_command_gives_up_its_first_line() {
        assert_eq!(
            from_command("printf 'hunter2\\nnoise\\n'").unwrap(),
            "hunter2"
        );
    }

    #[test]
    fn a_password_command_that_fails_is_reported_as_such() {
        let err = from_command("exit 3").unwrap_err();
        assert!(format!("{err:#}").contains("password command failed"));
    }

    #[test]
    fn a_password_command_that_prints_nothing_is_an_error() {
        let err = from_command("true").unwrap_err();
        assert!(format!("{err:#}").contains("printed nothing"));
    }
}
