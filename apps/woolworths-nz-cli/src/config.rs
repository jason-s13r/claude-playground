//! On-disk configuration and state.
//!
//! Config (the store you shop at) lives in
//! `~/.config/woolworths-nz-cli/config.toml`; the cached guest token and the
//! account session live under `~/.local/state/woolworths-nz-cli/`. Both roots
//! are overridable so tests -- and anyone with an unusual setup -- can redirect
//! them.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

pub const APP_NAME: &str = "woolworths-nz-cli";

#[derive(Clone, Debug)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Paths> {
        let dirs = directories::ProjectDirs::from("", "", APP_NAME);
        let config_dir = match env::var_os("WWNZ_CONFIG_DIR") {
            Some(v) => PathBuf::from(v),
            None => dirs
                .as_ref()
                .map(|d| d.config_dir().to_path_buf())
                .context("could not determine a config directory; set WWNZ_CONFIG_DIR")?,
        };
        let state_dir = match env::var_os("WWNZ_STATE_DIR") {
            Some(v) => PathBuf::from(v),
            None => dirs
                .as_ref()
                .and_then(|d| d.state_dir().map(|p| p.to_path_buf()))
                .or_else(|| dirs.as_ref().map(|d| d.data_dir().to_path_buf()))
                .context("could not determine a state directory; set WWNZ_STATE_DIR")?,
        };
        Ok(Paths {
            config_dir,
            state_dir,
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// The anonymous browse token, cached apart from the account session: the
    /// two authorise different things, so one must never be served in place of
    /// the other.
    pub fn guest_token_file(&self) -> PathBuf {
        self.state_dir.join("guest-token.json")
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// The pickup location prices and stock are quoted against. Woolworths
    /// prices per store, so this has to be chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
    /// The store's name, remembered alongside the id so `wwnz store show` can
    /// name it without a round trip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_name: Option<String>,
    /// Optional shell command printing the account password on stdout, so a
    /// session that has lapsed can be renewed without a prompt. The password
    /// itself is never stored here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_command: Option<String>,
}

impl Config {
    pub fn load(paths: &Paths) -> Result<Config> {
        let file = paths.config_file();
        match fs::read_to_string(&file) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("parsing {}", file.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", file.display())),
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let file = paths.config_file();
        fs::create_dir_all(&paths.config_dir)
            .with_context(|| format!("creating {}", paths.config_dir.display()))?;
        let text = toml::to_string_pretty(self).context("serialising config")?;
        fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
        restrict(&file);
        Ok(())
    }

    /// The store to price against: `--store`, then the environment, then the
    /// config file.
    pub fn store_id(&self, flag: Option<&str>) -> Option<String> {
        let from_env = env::var("WWNZ_STORE_ID")
            .ok()
            .filter(|v| !v.trim().is_empty());
        flag.map(str::to_string)
            .or(from_env)
            .or_else(|| self.store_id.clone())
            .filter(|s| !s.trim().is_empty())
    }
}

/// Best-effort 0600. A missing chmod is not worth failing a command over.
pub fn restrict(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let c = Config {
            store_id: Some("9195".into()),
            store_name: Some("Whangarei Woolworths".into()),
            password_command: None,
        };
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.store_id(None).as_deref(), Some("9195"));
        assert_eq!(back.store_name.as_deref(), Some("Whangarei Woolworths"));
    }

    #[test]
    fn unset_fields_are_not_written_out() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(!text.contains("store_id"), "got: {text}");
    }

    #[test]
    fn the_store_flag_beats_the_config_file() {
        let c = Config {
            store_id: Some("from-config".into()),
            ..Default::default()
        };
        assert_eq!(c.store_id(Some("from-flag")).as_deref(), Some("from-flag"));
        // A blank flag is not a choice, so it falls through.
        assert_eq!(c.store_id(Some("  ")), None);
    }
}
