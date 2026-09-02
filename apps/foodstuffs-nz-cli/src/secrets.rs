//! Somewhere to keep a login that is not a plaintext file in a repo.
//!
//! Preference is for the operating system's own credential store -- Keychain,
//! Credential Manager, Secret Service, whichever this platform provides -- via
//! the `keyring` crate, which papers over the differences. Where no such store
//! is reachable (a headless box with no Secret Service, say) it falls back to a
//! 0600 file under the state directory, and says so rather than pretending.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

const SERVICE: &str = "foodstuffs-nz-cli";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// The platform credential store.
    Keyring,
    /// A 0600 file, for platforms without one.
    File,
}

impl Backend {
    pub fn describe(self) -> &'static str {
        match self {
            Backend::Keyring => "the system credential store",
            Backend::File => "a 0600 file in the state directory (no system credential store)",
        }
    }
}

pub struct Secrets {
    backend: Backend,
    dir: PathBuf,
}

impl Secrets {
    pub fn new(state_dir: PathBuf) -> Secrets {
        // The override keeps tests off the developer's real credential store.
        let backend = match std::env::var("FSNZ_SECRET_BACKEND").ok().as_deref() {
            Some("file") => Backend::File,
            Some("keyring") => Backend::Keyring,
            _ if keyring::Entry::store_status().is_ok() => Backend::Keyring,
            _ => Backend::File,
        };
        Secrets {
            backend,
            dir: state_dir.join("secrets"),
        }
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn get(&self, account: &str) -> Result<Option<String>> {
        match self.backend {
            Backend::Keyring => match keyring::Entry::new(SERVICE, account)
                .context("opening the system credential store")?
                .get_password()
            {
                Ok(secret) => Ok(Some(secret).filter(|s| !s.trim().is_empty())),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(e).context("reading from the system credential store"),
            },
            Backend::File => match fs::read_to_string(self.path(account)) {
                Ok(s) => Ok(Some(s.trim().to_string()).filter(|s| !s.is_empty())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e).context("reading the stored login"),
            },
        }
    }

    pub fn set(&self, account: &str, secret: &str) -> Result<()> {
        match self.backend {
            Backend::Keyring => keyring::Entry::new(SERVICE, account)
                .context("opening the system credential store")?
                .set_password(secret)
                .context("writing to the system credential store"),
            Backend::File => {
                fs::create_dir_all(&self.dir)
                    .with_context(|| format!("creating {}", self.dir.display()))?;
                let path = self.path(account);
                fs::write(&path, secret).with_context(|| format!("writing {}", path.display()))?;
                crate::config::restrict(&path);
                Ok(())
            }
        }
    }

    /// Removing something that was never there is a success, not an error.
    pub fn delete(&self, account: &str) -> Result<bool> {
        match self.backend {
            Backend::Keyring => match keyring::Entry::new(SERVICE, account)
                .context("opening the system credential store")?
                .delete_credential()
            {
                Ok(()) => Ok(true),
                Err(keyring::Error::NoEntry) => Ok(false),
                Err(e) => Err(e).context("deleting from the system credential store"),
            },
            Backend::File => match fs::remove_file(self.path(account)) {
                Ok(()) => Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(e).context("removing the stored login"),
            },
        }
    }

    /// Account names reach the filesystem in the fallback backend, so they are
    /// reduced to something that cannot climb out of the directory.
    fn path(&self, account: &str) -> PathBuf {
        let safe: String = account
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        self.dir.join(safe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn file_store(dir: &TempDir) -> Secrets {
        std::env::set_var("FSNZ_SECRET_BACKEND", "file");
        Secrets::new(dir.path().to_path_buf())
    }

    #[test]
    fn round_trips_a_secret() {
        let dir = TempDir::new().unwrap();
        let s = file_store(&dir);
        assert_eq!(s.get("clubplus").unwrap(), None);
        s.set("clubplus", "a-stored-session").unwrap();
        assert_eq!(
            s.get("clubplus").unwrap().as_deref(),
            Some("a-stored-session")
        );
        assert!(s.delete("clubplus").unwrap());
        assert_eq!(s.get("clubplus").unwrap(), None);
    }

    #[test]
    fn deleting_nothing_is_not_an_error() {
        let dir = TempDir::new().unwrap();
        assert!(!file_store(&dir).delete("absent").unwrap());
    }

    #[test]
    fn account_names_cannot_escape_the_directory() {
        let dir = TempDir::new().unwrap();
        let s = file_store(&dir);
        assert_eq!(s.path("../../etc/passwd").parent(), Some(s.dir.as_path()));
    }

    #[test]
    fn the_fallback_is_named_honestly() {
        assert!(Backend::File
            .describe()
            .contains("no system credential store"));
    }
}
