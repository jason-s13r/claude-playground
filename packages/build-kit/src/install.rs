//! The record an update leaves behind, so a later `--version` can say where
//! this binary came from.

use std::path::PathBuf;

use net_kit::Paths;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The running binary, with symlinks resolved.
///
/// An update replaces the real file, not the link somebody put on their PATH.
pub fn exe_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.canonicalize().unwrap_or(exe))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Install {
    pub version: String,
    pub tag: String,
    /// The release page, for anyone wanting the notes.
    pub url: String,
    pub asset: String,
    /// Which file was replaced. A marker naming some other path belongs to a
    /// different copy of the tool and says nothing about this one.
    pub path: PathBuf,
    pub installed_at: u64,
}

impl Install {
    pub fn file(paths: &Paths) -> PathBuf {
        paths.state_file("install.json")
    }

    pub fn load(paths: &Paths) -> Option<Install> {
        let text = std::fs::read_to_string(Install::file(paths)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(&paths.state_dir)
            .map_err(|e| Error::io(format!("creating {}", paths.state_dir.display()), e))?;
        let file = Install::file(paths);
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Archive(format!("serialising the install record: {e}")))?;
        std::fs::write(&file, text).map_err(|e| Error::io(format!("writing {}", file.display()), e))
    }

    /// The record for *this* binary, if there is one.
    ///
    /// A marker whose path is not the running executable was left by a
    /// different copy, and claiming it would attribute this binary to an
    /// install it had nothing to do with.
    pub fn current(paths: &Paths) -> Option<Install> {
        let install = Install::load(paths)?;
        (Some(&install.path) == exe_path().as_ref()).then_some(install)
    }

    pub fn note<'a>(&'a self, tool: &'a str) -> crate::stamp::InstallNote<'a> {
        crate::stamp::InstallNote {
            tool,
            tag: &self.tag,
            installed_at: self.installed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(path: PathBuf) -> Install {
        Install {
            version: "1.2.3".into(),
            tag: "grocery-nz-cli/v1.2.3".into(),
            url: "https://example.test/releases/v1.2.3".into(),
            asset: "grocery-nz-cli-1.2.3-darwin-arm64.tar.gz".into(),
            path,
            installed_at: 1_756_512_000,
        }
    }

    #[test]
    fn round_trips_through_the_state_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let paths = Paths::new(dir.path().join("cfg"), dir.path().join("state"));
        assert!(Install::load(&paths).is_none());
        record("/usr/local/bin/gsnz".into()).save(&paths).unwrap();
        assert_eq!(Install::load(&paths).unwrap().tag, "grocery-nz-cli/v1.2.3");
    }

    #[test]
    fn a_marker_for_another_binary_is_not_claimed() {
        let dir = tempfile::TempDir::new().unwrap();
        let paths = Paths::new(dir.path().join("cfg"), dir.path().join("state"));
        record("/somewhere/else/gsnz".into()).save(&paths).unwrap();
        // The record exists, but it is not about the binary running the test.
        assert!(Install::load(&paths).is_some());
        assert!(Install::current(&paths).is_none());
    }

    #[test]
    fn the_running_binary_claims_its_own_marker() {
        let dir = tempfile::TempDir::new().unwrap();
        let paths = Paths::new(dir.path().join("cfg"), dir.path().join("state"));
        record(exe_path().unwrap()).save(&paths).unwrap();
        assert!(Install::current(&paths).is_some());
    }
}
