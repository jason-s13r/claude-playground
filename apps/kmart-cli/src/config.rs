//! `~/.config/kmart-cli/config.toml` -- the settings worth keeping.
//!
//! Everything here is optional and everything has a flag or an environment
//! variable that beats it. The file exists so that `kmart use nz` is
//! remembered, not as a second way to configure the program.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Which country's Kmart to talk to.
    ///
    /// The setting this program is mostly about. It picks the catalogue, the
    /// prices, the currency and the gateway host, so it changes what a command
    /// *returns* rather than how it is shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<kmart_api::Country>,
    /// The postcode stock is quoted for.
    ///
    /// Not a display preference either: every stock answer is relative to a
    /// postcode, and without one there is nothing to ask about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postcode: Option<String>,
    /// Which island a New Zealand listing is filtered to, `NI` or `SI`.
    ///
    /// Kmart ranges differently across the strait, so this changes what a
    /// listing contains. Australia has no equivalent facet and ignores it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub island: Option<String>,
    /// The store to care about, by location id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
    /// The Constructor.io visitor id.
    ///
    /// Kept rather than generated per run because the index personalises on
    /// it: a new one every time is a new visitor every time, which is both
    /// worse ranking and a louder pattern than a returning one. Written on
    /// first use; there is no reason to set it by hand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visitor_id: Option<String>,
    // Only written when changed. A file listing every default is one nobody
    // can skim, and this is still a file people edit by hand.
    #[serde(skip_serializing_if = "is_default")]
    pub auth: Auth,
    #[serde(skip_serializing_if = "is_default")]
    pub output: Output,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Auth {
    /// A shell command that prints the password, for a password manager. Beats
    /// the stored one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_command: Option<String>,
    /// Whether `auth login` keeps the password.
    ///
    /// Worth having, but less load-bearing here than for the other tools in
    /// this repo: Kmart issues a refresh token, so a lapsed session renews
    /// without a password. The password is only needed when that token is
    /// itself refused.
    pub store_password: bool,
}

impl Default for Auth {
    fn default() -> Auth {
        Auth {
            password_command: None,
            store_password: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Output {
    pub color: ColorChoice,
}

/// One setting, for `config list`.
pub struct Setting {
    pub key: &'static str,
    pub what: &'static str,
}

/// Every key `config get` and `config set` accept, and what each does.
pub const SETTINGS: [Setting; 7] = [
    Setting {
        key: "country",
        what: "which Kmart to talk to: au or nz",
    },
    Setting {
        key: "postcode",
        what: "the postcode stock is quoted for",
    },
    Setting {
        key: "island",
        what: "filter New Zealand listings to NI or SI",
    },
    Setting {
        key: "store_id",
        what: "the store to care about, by location id",
    },
    Setting {
        key: "auth.password_command",
        what: "a command that prints the password",
    },
    Setting {
        key: "auth.store_password",
        what: "whether `auth login` keeps the password",
    },
    Setting {
        key: "output.color",
        what: "auto, always or never",
    },
];

impl Config {
    pub fn load(path: &Path) -> AppResult<Config> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| AppError::usage(format!("the config could not be written: {e}")))?;
        std::fs::write(path, text)?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> AppResult<Option<String>> {
        Ok(match key {
            "country" => self.country.map(|c| c.to_string()),
            "postcode" => self.postcode.clone(),
            "island" => self.island.clone(),
            "store_id" => self.store_id.clone(),
            "auth.password_command" => self.auth.password_command.clone(),
            "auth.store_password" => Some(self.auth.store_password.to_string()),
            "output.color" => Some(format!("{:?}", self.output.color).to_lowercase()),
            _ => return Err(unknown(key)),
        })
    }

    /// Refused here rather than at the next command: a value that will not
    /// parse is better rejected while the person is still looking at it.
    pub fn set(&mut self, key: &str, value: &str) -> AppResult<()> {
        match key {
            "country" => {
                self.country = Some(kmart_api::Country::parse(value).ok_or_else(|| {
                    AppError::usage(format!("{value:?} is not a country; use `au` or `nz`"))
                })?)
            }
            "postcode" => self.postcode = Some(value.trim().to_string()),
            "island" => self.island = Some(parse_island(value)?),
            "store_id" => self.store_id = Some(value.trim().to_string()),
            "auth.password_command" => self.auth.password_command = Some(value.to_string()),
            "auth.store_password" => self.auth.store_password = parse_bool(value)?,
            "output.color" => {
                self.output.color = match value.trim().to_lowercase().as_str() {
                    "auto" => ColorChoice::Auto,
                    "always" => ColorChoice::Always,
                    "never" => ColorChoice::Never,
                    _ => {
                        return Err(AppError::usage(format!(
                            "{value:?} is not a colour choice; use auto, always or never"
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
            "country" => self.country = None,
            "postcode" => self.postcode = None,
            "island" => self.island = None,
            "store_id" => self.store_id = None,
            "auth.password_command" => self.auth.password_command = None,
            "auth.store_password" => self.auth.store_password = Auth::default().store_password,
            "output.color" => self.output.color = ColorChoice::default(),
            _ => return Err(unknown(key)),
        }
        Ok(())
    }
}

/// `NI` or `SI`, however it was typed.
pub fn parse_island(value: &str) -> AppResult<String> {
    match value.trim().to_lowercase().as_str() {
        "ni" | "north" | "north island" => Ok("NI".into()),
        "si" | "south" | "south island" => Ok("SI".into()),
        _ => Err(AppError::usage(format!(
            "{value:?} is not an island; use `north` or `south`"
        ))),
    }
}

fn parse_bool(value: &str) -> AppResult<bool> {
    match value.trim().to_lowercase().as_str() {
        "true" | "yes" | "1" | "on" => Ok(true),
        "false" | "no" | "0" | "off" => Ok(false),
        _ => Err(AppError::usage(format!("{value:?} is not true or false"))),
    }
}

fn unknown(key: &str) -> AppError {
    let known = SETTINGS
        .iter()
        .map(|s| s.key)
        .collect::<Vec<_>>()
        .join(", ");
    AppError::usage(format!("no setting called {key:?}; there is: {known}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_the_default_config_not_an_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = Config::load(&dir.path().join("nope.toml")).unwrap();
        assert!(config.country.is_none());
        assert!(config.auth.store_password, "the default is to keep it");
    }

    #[test]
    fn every_listed_setting_round_trips_through_get_and_set() {
        // The table and the match arms are written twice by hand; this is what
        // catches one of them being extended without the other.
        let mut config = Config::default();
        for setting in SETTINGS {
            let value = match setting.key {
                "country" => "nz",
                "postcode" => "1010",
                "island" => "north",
                "store_id" => "8229",
                "auth.password_command" => "pass show kmart",
                "auth.store_password" => "false",
                "output.color" => "never",
                other => panic!("{other} has no test value"),
            };
            config.set(setting.key, value).unwrap();
            assert!(
                config.get(setting.key).unwrap().is_some(),
                "{} reads back",
                setting.key
            );
            config.unset(setting.key).unwrap();
        }
    }

    #[test]
    fn an_island_is_stored_as_the_code_the_index_filters_on() {
        let mut config = Config::default();
        config.set("island", "south").unwrap();
        assert_eq!(config.island.as_deref(), Some("SI"));
        config.set("island", "NI").unwrap();
        assert_eq!(config.island.as_deref(), Some("NI"));
        assert!(config.set("island", "middle").is_err());
    }

    #[test]
    fn a_value_that_will_not_parse_is_refused_now_rather_than_later() {
        let mut config = Config::default();
        assert!(config.set("country", "uk").is_err());
        assert!(config.set("output.color", "purple").is_err());
        assert!(config.set("auth.store_password", "maybe").is_err());
        assert!(config.set("nonsense", "x").is_err());
    }

    #[test]
    fn an_unknown_key_lists_the_ones_that_exist() {
        let e = Config::default().get("colour").unwrap_err();
        assert!(format!("{e}").contains("output.color"), "{e}");
    }

    #[test]
    fn only_what_was_changed_is_written() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.set("country", "au").unwrap();
        config.save(&path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("country = \"au\""), "{text}");
        // A file listing every default is one nobody can skim.
        assert!(!text.contains("store_password"), "{text}");
        assert_eq!(Config::load(&path).unwrap().country, config.country);
    }
}
