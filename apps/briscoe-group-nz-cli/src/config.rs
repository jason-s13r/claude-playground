//! The config file: what a bare command does when no flag says otherwise.
//!
//! One file for both fascias, with `banner` deciding which a bare command
//! talks to. The credentials are *not* here -- those live in the credential
//! store, one entry per fascia, because there is no SSO between them.

use std::path::Path;

use bgnz_api::Banner;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Which fascia a bare command talks to. `bgnz -b rebel` overrides it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Banner>,
    /// The store a bare `bgnz stock` asks about, per fascia -- the two have
    /// different shops and different ids, so one setting could not serve both.
    #[serde(skip_serializing_if = "Stores::is_empty")]
    pub store: Stores,
    #[serde(skip_serializing_if = "Output::is_default")]
    pub output: Output,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Stores {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub briscoes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebelsport: Option<i64>,
}

impl Stores {
    fn is_empty(&self) -> bool {
        self.briscoes.is_none() && self.rebelsport.is_none()
    }

    pub fn get(&self, banner: Banner) -> Option<i64> {
        match banner {
            Banner::Briscoes => self.briscoes,
            Banner::RebelSport => self.rebelsport,
        }
    }

    pub fn set(&mut self, banner: Banner, store: Option<i64>) {
        match banner {
            Banner::Briscoes => self.briscoes = store,
            Banner::RebelSport => self.rebelsport = store,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Output {
    pub color: ColorChoice,
}

impl Output {
    fn is_default(&self) -> bool {
        *self == Output::default()
    }
}

/// The settings `config set` accepts, and what each is for.
pub const KEYS: [&str; 4] = [
    "banner",
    "store.briscoes",
    "store.rebelsport",
    "output.color",
];

pub fn describe(key: &str) -> &'static str {
    match key {
        "banner" => "the fascia a command talks to when -b is not given",
        "store.briscoes" => "the Briscoes store `stock` asks about, by store id",
        "store.rebelsport" => "the Rebel Sport store `stock` asks about, by store id",
        "output.color" => "auto, always or never",
        _ => "",
    }
}

impl Config {
    pub fn load(file: &Path) -> AppResult<Config> {
        Ok(net_kit::config::load_toml(file)?)
    }

    pub fn save(&self, file: &Path) -> AppResult<()> {
        Ok(net_kit::config::save_toml(file, self)?)
    }

    pub fn get(&self, key: &str) -> AppResult<Option<String>> {
        Ok(match key {
            "banner" => self.banner.map(|b| b.id().to_string()),
            "store.briscoes" => self.store.briscoes.map(|s| s.to_string()),
            "store.rebelsport" => self.store.rebelsport.map(|s| s.to_string()),
            "output.color" => Some(
                match self.output.color {
                    ColorChoice::Auto => "auto",
                    ColorChoice::Always => "always",
                    ColorChoice::Never => "never",
                }
                .to_string(),
            ),
            _ => return Err(unknown(key)),
        })
    }

    pub fn set(&mut self, key: &str, value: &str) -> AppResult<()> {
        match key {
            "banner" => {
                let banner = Banner::parse(value).ok_or_else(|| {
                    AppError::usage(format!(
                        "{value:?} is not a fascia; use `briscoes` or `rebel`"
                    ))
                })?;
                self.banner = Some(banner);
            }
            "store.briscoes" | "store.rebelsport" => {
                let id: i64 = value.trim().parse().map_err(|_| {
                    AppError::usage(format!(
                        "{value:?} is not a store id; `bgnz stores` lists them"
                    ))
                })?;
                let banner = if key.ends_with("briscoes") {
                    Banner::Briscoes
                } else {
                    Banner::RebelSport
                };
                self.store.set(banner, Some(id));
            }
            "output.color" => {
                self.output.color = match value.trim().to_lowercase().as_str() {
                    "auto" => ColorChoice::Auto,
                    "always" => ColorChoice::Always,
                    "never" => ColorChoice::Never,
                    _ => {
                        return Err(AppError::usage(format!(
                            "{value:?} is not a colour setting; use auto, always or never"
                        )))
                    }
                }
            }
            _ => return Err(unknown(key)),
        }
        Ok(())
    }

    pub fn unset(&mut self, key: &str) -> AppResult<()> {
        match key {
            "banner" => self.banner = None,
            "store.briscoes" => self.store.briscoes = None,
            "store.rebelsport" => self.store.rebelsport = None,
            "output.color" => self.output.color = ColorChoice::Auto,
            _ => return Err(unknown(key)),
        }
        Ok(())
    }
}

fn unknown(key: &str) -> AppError {
    AppError::usage(format!(
        "there is no setting called {key:?}; try one of {}",
        KEYS.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_accepts_the_spellings_people_type() {
        let mut c = Config::default();
        c.set("banner", "rebel").expect("rebel is a fascia");
        assert_eq!(c.banner, Some(Banner::RebelSport));
        assert_eq!(
            c.get("banner").expect("readable").as_deref(),
            Some("rebelsport")
        );
        assert!(c.set("banner", "kmart").is_err());
    }

    #[test]
    fn each_fascia_remembers_its_own_store() {
        // The two have different shops with different ids; one setting could
        // not serve both, and sharing one would send `stock` to the wrong shop.
        let mut c = Config::default();
        c.set("store.briscoes", "291").expect("a store id");
        c.set("store.rebelsport", "444").expect("a store id");
        assert_eq!(c.store.get(Banner::Briscoes), Some(291));
        assert_eq!(c.store.get(Banner::RebelSport), Some(444));

        c.unset("store.briscoes").expect("unsets");
        assert_eq!(c.store.get(Banner::Briscoes), None);
        assert_eq!(c.store.get(Banner::RebelSport), Some(444), "untouched");
    }

    #[test]
    fn a_store_must_be_a_number() {
        let mut c = Config::default();
        assert!(c.set("store.briscoes", "Albany").is_err());
    }

    #[test]
    fn an_unknown_setting_says_what_the_settings_are() {
        let mut c = Config::default();
        let e = c.set("colour", "always").expect_err("no such setting");
        assert!(e.to_string().contains("output.color"), "{e}");
    }

    #[test]
    fn a_saved_config_round_trips() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("config.toml");
        let mut c = Config::default();
        c.set("banner", "rebel").expect("sets");
        c.set("store.rebelsport", "444").expect("sets");
        c.save(&file).expect("saves");

        let text = std::fs::read_to_string(&file).expect("reads");
        assert!(text.contains("banner = \"rebelsport\""), "{text}");
        let back = Config::load(&file).expect("loads");
        assert_eq!(back.banner, Some(Banner::RebelSport));
        assert_eq!(back.store.get(Banner::RebelSport), Some(444));
    }

    #[test]
    fn a_missing_config_is_the_default_rather_than_a_failure() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let c = Config::load(&dir.path().join("nothing.toml")).expect("a first run has no config");
        assert_eq!(c.banner, None);
    }
}
