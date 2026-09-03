//! Somewhere to keep a login that is not a plaintext file in a repo.
//!
//! Preference is the operating system's own credential store -- Keychain,
//! Credential Manager, Secret Service -- via `keyring`, which papers over the
//! differences. Where none is reachable (a headless box with no Secret
//! Service) it falls back to a 0600 file under the state directory, and says
//! so rather than pretending.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::paths::restrict;

/// Reduce a name to something that is one path segment and cannot escape it.
fn safe(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// The platform credential store.
    Keyring,
    /// A 0600 file, for platforms without one.
    File,
}

impl Backend {
    /// What this platform offers. The caller decides whether to override it --
    /// tests always do, to stay off the developer's real credential store.
    pub fn detect() -> Backend {
        if keyring::Entry::store_status().is_ok() {
            Backend::Keyring
        } else {
            Backend::File
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Backend::Keyring => "the system credential store",
            Backend::File => "a 0600 file in the state directory (no system credential store)",
        }
    }
}

/// `Clone` because a client that can re-authenticate has to carry one.
#[derive(Clone, Debug)]
pub struct Secrets {
    service: String,
    backend: Backend,
    dir: PathBuf,
}

impl Secrets {
    /// `service` names the tool in the credential store, and is why two CLIs
    /// on one machine do not read each other's logins.
    pub fn new(service: impl Into<String>, backend: Backend, state_dir: &Path) -> Secrets {
        Secrets {
            service: service.into(),
            backend,
            dir: state_dir.join("secrets"),
        }
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    pub fn get(&self, account: &str) -> Result<Option<String>> {
        match self.backend {
            Backend::Keyring => match self.entry(account)?.get_password() {
                Ok(secret) => Ok(Some(secret).filter(|s| !s.trim().is_empty())),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(Error::keyring("reading from the credential store", e)),
            },
            Backend::File => match fs::read_to_string(self.path(account)) {
                Ok(s) => Ok(Some(s.trim().to_string()).filter(|s| !s.is_empty())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(Error::io("reading the stored secret", e)),
            },
        }
    }

    pub fn set(&self, account: &str, secret: &str) -> Result<()> {
        match self.backend {
            Backend::Keyring => self
                .entry(account)?
                .set_password(secret)
                .map_err(|e| Error::keyring("writing to the credential store", e)),
            Backend::File => {
                let path = self.path(account);
                // The service is a directory level, so `self.dir` alone is not
                // enough to create.
                let parent = path.parent().unwrap_or(&self.dir);
                fs::create_dir_all(parent)
                    .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
                fs::write(&path, secret)
                    .map_err(|e| Error::io(format!("writing {}", path.display()), e))?;
                restrict(&path);
                Ok(())
            }
        }
    }

    /// Removing something that was never there is a success, not an error.
    pub fn delete(&self, account: &str) -> Result<bool> {
        match self.backend {
            Backend::Keyring => match self.entry(account)?.delete_credential() {
                Ok(()) => Ok(true),
                Err(keyring::Error::NoEntry) => Ok(false),
                Err(e) => Err(Error::keyring("deleting from the credential store", e)),
            },
            Backend::File => match fs::remove_file(self.path(account)) {
                Ok(()) => Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(Error::io("removing the stored secret", e)),
            },
        }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, account)
            .map_err(|e| Error::keyring("opening the credential store", e))
    }

    /// Both names reach the filesystem in the fallback backend, so both are
    /// reduced to something that cannot climb out of the directory.
    ///
    /// The service is part of the path so that two tools pointed at one state
    /// directory cannot read each other's secrets -- the keyring backend
    /// separates them by service, and the fallback should not be weaker.
    fn path(&self, account: &str) -> PathBuf {
        self.dir.join(safe(&self.service)).join(safe(account))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// No `set_var` anywhere: the backend is an argument, so these tests can
    /// run in parallel and never touch a real credential store.
    fn file_store(dir: &TempDir) -> Secrets {
        Secrets::new("net-kit-test", Backend::File, dir.path())
    }

    #[test]
    fn round_trips_a_secret() {
        let dir = TempDir::new().unwrap();
        let s = file_store(&dir);
        assert_eq!(s.get("session").unwrap(), None);
        s.set("session", "a-stored-session").unwrap();
        assert_eq!(
            s.get("session").unwrap().as_deref(),
            Some("a-stored-session")
        );
        assert!(s.delete("session").unwrap());
        assert_eq!(s.get("session").unwrap(), None);
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
        let inside = s.dir.join("net_kit_test");
        assert_eq!(s.path("../../etc/passwd").parent(), Some(inside.as_path()));
        assert_eq!(s.path("a/b").parent(), Some(inside.as_path()));
    }

    #[test]
    fn two_tools_sharing_a_state_directory_stay_separate() {
        let dir = TempDir::new().unwrap();
        let a = Secrets::new("tool-a", Backend::File, dir.path());
        let b = Secrets::new("tool-b", Backend::File, dir.path());
        a.set("session", "from-a").unwrap();
        assert_eq!(b.get("session").unwrap(), None);
        b.set("session", "from-b").unwrap();
        assert_eq!(a.get("session").unwrap().as_deref(), Some("from-a"));
    }

    #[test]
    fn the_fallback_is_named_honestly() {
        assert!(Backend::File
            .describe()
            .contains("no system credential store"));
    }
}
