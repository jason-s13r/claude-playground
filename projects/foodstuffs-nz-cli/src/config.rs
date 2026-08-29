//! On-disk configuration and state.
//!
//! Config (the store you shop at, the banner you default to) lives in
//! `~/.config/foodstuffs-nz-cli/config.toml`; cached guest tokens live under
//! `~/.local/state/foodstuffs-nz-cli/`. Both roots are overridable so tests --
//! and anyone with an unusual setup -- can redirect them.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::banner::Banner;

pub const APP_NAME: &str = "foodstuffs-nz-cli";

#[derive(Clone, Debug)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Paths> {
        let dirs = directories::ProjectDirs::from("", "", APP_NAME);
        let config_dir = match env::var_os("FSNZ_CONFIG_DIR") {
            Some(v) => PathBuf::from(v),
            None => dirs
                .as_ref()
                .map(|d| d.config_dir().to_path_buf())
                .context("could not determine a config directory; set FSNZ_CONFIG_DIR")?,
        };
        let state_dir = match env::var_os("FSNZ_STATE_DIR") {
            Some(v) => PathBuf::from(v),
            None => dirs
                .as_ref()
                .and_then(|d| d.state_dir().map(|p| p.to_path_buf()))
                .or_else(|| dirs.as_ref().map(|d| d.data_dir().to_path_buf()))
                .context("could not determine a state directory; set FSNZ_STATE_DIR")?,
        };
        Ok(Paths {
            config_dir,
            state_dir,
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn token_file(&self, banner: Banner) -> PathBuf {
        self.state_dir.join(banner.id()).join("token.json")
    }

    /// Guest tokens are cached apart from account ones: they authorise
    /// different things, so one must never be served in place of the other.
    pub fn guest_token_file(&self, banner: Banner) -> PathBuf {
        self.state_dir.join(banner.id()).join("guest-token.json")
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Banner used when `--banner` is not given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    /// Optional shell command printing the Club Plus password on stdout, so a
    /// session that has lapsed can be renewed without a prompt. The password
    /// itself is never stored here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_command: Option<String>,
    #[serde(default, skip_serializing_if = "BannerConfig::is_empty")]
    pub newworld: BannerConfig,
    #[serde(default, skip_serializing_if = "BannerConfig::is_empty")]
    pub paknsave: BannerConfig,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BannerConfig {
    /// Prices and availability are per store, so this has to be chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
    /// Optional shell command printing a guest token on stdout. An escape
    /// hatch for when the storefront refuses this tool's own request but
    /// happily answers `curl`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_command: Option<String>,
}

impl BannerConfig {
    fn is_empty(&self) -> bool {
        self.store_id.is_none() && self.token_command.is_none()
    }
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

    pub fn for_banner(&self, banner: Banner) -> &BannerConfig {
        match banner {
            Banner::NewWorld => &self.newworld,
            Banner::PaknSave => &self.paknsave,
        }
    }

    pub fn for_banner_mut(&mut self, banner: Banner) -> &mut BannerConfig {
        match banner {
            Banner::NewWorld => &mut self.newworld,
            Banner::PaknSave => &mut self.paknsave,
        }
    }

    /// Which banner to use with no `--banner`: the environment, then the
    /// config file, then New World.
    pub fn default_banner(&self) -> Result<Banner> {
        if let Ok(v) = env::var("FSNZ_BANNER") {
            if !v.trim().is_empty() {
                return Banner::parse(&v);
            }
        }
        match self.banner.as_deref() {
            Some(v) if !v.trim().is_empty() => Banner::parse(v),
            _ => Ok(Banner::NewWorld),
        }
    }

    /// The store to price against: `--store`, then the environment, then the
    /// config file.
    pub fn store_id(&self, banner: Banner, flag: Option<&str>) -> Option<String> {
        let from_flag = flag.map(str::to_string);
        let env_key = match banner {
            Banner::NewWorld => "FSNZ_NEWWORLD_STORE_ID",
            Banner::PaknSave => "FSNZ_PAKNSAVE_STORE_ID",
        };
        let from_env = env::var(env_key).ok().filter(|v| !v.trim().is_empty());
        from_flag
            .or(from_env)
            .or_else(|| self.for_banner(banner).store_id.clone())
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
        let mut c = Config {
            banner: Some("paknsave".into()),
            ..Default::default()
        };
        c.for_banner_mut(Banner::PaknSave).store_id = Some("abc-123".into());
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.default_banner().unwrap(), Banner::PaknSave);
        assert_eq!(
            back.store_id(Banner::PaknSave, None).as_deref(),
            Some("abc-123")
        );
        assert_eq!(back.store_id(Banner::NewWorld, None), None);
    }

    #[test]
    fn empty_banner_tables_are_not_written_out() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(!text.contains("newworld"), "got: {text}");
    }

    #[test]
    fn the_store_flag_beats_the_config_file() {
        let mut c = Config::default();
        c.for_banner_mut(Banner::NewWorld).store_id = Some("from-config".into());
        assert_eq!(
            c.store_id(Banner::NewWorld, Some("from-flag")).as_deref(),
            Some("from-flag")
        );
    }
}
