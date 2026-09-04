//! `~/.config/woolworths-nz-cli/config.toml` -- the settings worth keeping.
//!
//! Everything here is optional and everything has a flag or an environment
//! variable that beats it. The file exists so that `wwnz store set` is
//! remembered, not as a second way to configure the program.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The store prices are quoted against. A local record of what the *cart*
    /// was bound to server-side by `store set`, kept so a listing can be headed
    /// with it without a round trip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
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
    /// A shell command that prints the password on stdout, for a password
    /// manager. Beats the stored one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_command: Option<String>,
    /// Whether `auth login` keeps the password. Signing in again is the only
    /// renewal a Woolworths session has, so without this a lapsed session stops
    /// every account command until someone signs in by hand.
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Output {
    pub color: ColorChoice,
}

impl Default for Output {
    fn default() -> Output {
        Output {
            color: ColorChoice::Auto,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

impl Config {
    pub fn load(file: &Path) -> AppResult<Config> {
        Ok(net_kit::config::load_toml(file)?)
    }
}

/// Every setting, as a dotted key.
///
/// The list is explicit rather than derived so that `config list` has an order
/// worth reading, and so a key that is no longer real fails loudly instead of
/// writing a field nothing reads.
pub const KEYS: [&str; 4] = [
    "store_id",
    "auth.password_command",
    "auth.store_password",
    "output.color",
];

/// What a key means, for `config list`.
pub fn describe(key: &str) -> &'static str {
    match key {
        "store_id" => "the store prices are quoted against; `store set` resolves a name",
        "auth.password_command" => "a command that prints the password, for a password manager",
        "auth.store_password" => {
            "keep the password at login, so a lapsed session can sign itself in again"
        }
        _ => "auto, always or never",
    }
}

impl Config {
    /// The value as it would be written, or `None` when nothing is set.
    pub fn get(&self, key: &str) -> AppResult<Option<String>> {
        Ok(match key {
            "store_id" => self.store_id.clone(),
            "auth.password_command" => self.auth.password_command.clone(),
            "auth.store_password" => Some(self.auth.store_password.to_string()),
            "output.color" => Some(
                match self.output.color {
                    ColorChoice::Auto => "auto",
                    ColorChoice::Always => "always",
                    ColorChoice::Never => "never",
                }
                .into(),
            ),
            _ => return Err(unknown(key)),
        })
    }

    /// Parse and store a value, so a bad one is refused now rather than at the
    /// next command that reads it.
    pub fn set(&mut self, key: &str, value: &str) -> AppResult<()> {
        let value = value.trim();
        match key {
            // Written straight through rather than resolved: `store set` is
            // what binds the cart, and this key exists so the record can be
            // corrected by hand.
            "store_id" => self.store_id = Some(value.to_string()),
            "auth.password_command" => self.auth.password_command = Some(value.to_string()),
            "auth.store_password" => self.auth.store_password = boolean(value)?,
            "output.color" => {
                self.output.color = match value.to_lowercase().as_str() {
                    "auto" => ColorChoice::Auto,
                    "always" | "yes" | "on" => ColorChoice::Always,
                    "never" | "no" | "off" => ColorChoice::Never,
                    _ => return Err(AppError::usage("color takes `auto`, `always` or `never`")),
                }
            }
            _ => return Err(unknown(key)),
        }
        Ok(())
    }

    /// Back to the default. Not the same as setting an empty string: an empty
    /// `password_command` would be run and would fail.
    pub fn unset(&mut self, key: &str) -> AppResult<()> {
        match key {
            "store_id" => self.store_id = None,
            "auth.password_command" => self.auth.password_command = None,
            "auth.store_password" => self.auth.store_password = Auth::default().store_password,
            "output.color" => self.output.color = ColorChoice::default(),
            _ => return Err(unknown(key)),
        }
        Ok(())
    }
}

fn boolean(value: &str) -> AppResult<bool> {
    match value.to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        _ => Err(AppError::usage(format!("{value:?} is not true or false"))),
    }
}

fn unknown(key: &str) -> AppError {
    AppError::usage(format!(
        "no setting called {key:?}. Run `wwnz config list` for the {} there are.",
        KEYS.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_a_default_config() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.store_id, None);
        assert!(cfg.auth.store_password);
    }

    #[test]
    fn a_saved_config_round_trips() {
        let cfg = Config {
            store_id: Some("9048".into()),
            ..Config::default()
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.store_id.as_deref(), Some("9048"));
    }

    #[test]
    fn a_typo_in_a_key_is_reported_rather_than_ignored() {
        // `deny_unknown_fields` is the point: a config that silently does
        // nothing is worse than one that refuses to load.
        let err = toml::from_str::<Config>("stroe_id = \"9048\"").unwrap_err();
        assert!(err.to_string().contains("stroe_id"), "{err}");
    }

    #[test]
    fn the_settings_of_the_older_flat_file_are_named_in_the_error() {
        // 0.2 kept `password_command` and `store_password` at the top level.
        // Refusing to load says so; silently ignoring them would leave a login
        // prompting for a password the file says how to fetch.
        let err = toml::from_str::<Config>("password_command = \"pass show ww\"").unwrap_err();
        assert!(err.to_string().contains("password_command"), "{err}");
    }

    #[test]
    fn a_bad_value_is_refused_at_the_point_of_writing_it() {
        let mut config = Config::default();
        assert!(config.set("output.color", "purple").is_err());
        assert!(config.set("auth.store_password", "maybe").is_err());
        assert!(config.set("output.color", "never").is_ok());
    }

    #[test]
    fn unset_restores_the_default_rather_than_emptying_it() {
        let mut config = Config::default();
        config.set("auth.store_password", "false").unwrap();
        config.unset("auth.store_password").unwrap();
        assert_eq!(
            config.get("auth.store_password").unwrap().as_deref(),
            Some("true")
        );
    }
}
