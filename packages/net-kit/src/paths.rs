//! Where a tool keeps its config and its state.
//!
//! Defaults come from the platform's own convention via `directories`.
//! Overrides are applied by the caller -- this crate does not read the
//! environment, so `GSNZ_CONFIG_DIR` is the app's business, not ours.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl Paths {
    /// Platform defaults for an application name.
    ///
    /// Not every platform has a state directory distinct from its data
    /// directory; where it does not, state lands in the data directory rather
    /// than failing.
    pub fn defaults(app_name: &str) -> Result<Paths> {
        let dirs = directories::ProjectDirs::from("", "", app_name);
        let config_dir = dirs
            .as_ref()
            .map(|d| d.config_dir().to_path_buf())
            .ok_or_else(|| {
                Error::Config(format!(
                    "could not determine a config directory for {app_name}"
                ))
            })?;
        let state_dir = dirs
            .as_ref()
            .and_then(|d| d.state_dir().map(Path::to_path_buf))
            .or_else(|| dirs.as_ref().map(|d| d.data_dir().to_path_buf()))
            .ok_or_else(|| {
                Error::Config(format!(
                    "could not determine a state directory for {app_name}"
                ))
            })?;
        Ok(Paths {
            config_dir,
            state_dir,
        })
    }

    /// Build from two known directories, for a caller that has both already.
    pub fn new(config_dir: PathBuf, state_dir: PathBuf) -> Paths {
        Paths {
            config_dir,
            state_dir,
        }
    }

    pub fn with_config_dir(mut self, dir: impl Into<PathBuf>) -> Paths {
        self.config_dir = dir.into();
        self
    }

    pub fn with_state_dir(mut self, dir: impl Into<PathBuf>) -> Paths {
        self.state_dir = dir.into();
        self
    }

    /// The same config directory, with state moved under a namespace.
    ///
    /// One tool talking to several retailers keeps their tokens apart: a New
    /// World token presented to PAK'nSAVE is not merely refused, it answers
    /// with someone else's empty cart.
    pub fn scoped(&self, ns: &str) -> Paths {
        Paths {
            config_dir: self.config_dir.clone(),
            state_dir: self.state_dir.join(ns),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn state_file(&self, name: &str) -> PathBuf {
        self.state_dir.join(name)
    }
}

/// Owner-only, best effort. Anything under the state directory may hold a
/// token; a failure here is not worth aborting a run over, and on platforms
/// without unix permissions there is nothing to do.
pub fn restrict(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_replace_only_what_they_name() {
        let paths = Paths::new("/cfg".into(), "/state".into()).with_state_dir("/elsewhere");
        assert_eq!(paths.config_dir, PathBuf::from("/cfg"));
        assert_eq!(paths.state_dir, PathBuf::from("/elsewhere"));
    }

    #[test]
    fn scoping_moves_state_and_leaves_config_alone() {
        let scoped = Paths::new("/cfg".into(), "/state".into()).scoped("newworld");
        assert_eq!(scoped.config_dir, PathBuf::from("/cfg"));
        assert_eq!(scoped.state_dir, PathBuf::from("/state/newworld"));
        assert_eq!(scoped.config_file(), PathBuf::from("/cfg/config.toml"));
    }

    #[test]
    fn defaults_give_two_usable_directories() {
        let paths = Paths::defaults("net-kit-test").unwrap();
        assert!(paths.config_dir.is_absolute());
        assert!(paths.state_dir.is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn restrict_makes_a_file_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("token.json");
        std::fs::write(&file, "secret").unwrap();
        restrict(&file);
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn restricting_a_missing_file_does_not_panic() {
        restrict(Path::new("/nonexistent/net-kit/definitely-not-here"));
    }
}
