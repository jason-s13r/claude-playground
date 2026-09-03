//! `~/.config/foodstuffs-nz-cli/config.toml` -- the settings worth keeping.
//!
//! Everything here is optional and everything has a flag or an environment
//! variable that beats it. The file exists so that `fsnz store set` and
//! `fsnz -b pns` are remembered, not as a second way to configure the program.

use serde::{Deserialize, Serialize};
use std::path::Path;

use gsnz_core::RetailerId;

use crate::error::{AppError, AppResult};
use crate::retailers::BANNERS;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Which banner a bare command talks to. `fsnz -b pns` overrides it.
    #[serde(rename = "banner", skip_serializing_if = "Option::is_none")]
    pub retailer: Option<RetailerId>,
    // Only written when changed. A file listing every default is one nobody
    // can skim, and this is still a file people edit by hand.
    #[serde(skip_serializing_if = "is_default")]
    pub compare: Compare,
    #[serde(skip_serializing_if = "is_default")]
    pub auth: Auth,
    #[serde(skip_serializing_if = "is_default")]
    pub output: Output,
    #[serde(skip_serializing_if = "is_default")]
    pub newworld: Retailer,
    #[serde(skip_serializing_if = "is_default")]
    pub paknsave: Retailer,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Compare {
    /// Which banners a bare `fsnz compare` spans, and in what column order.
    pub retailers: Vec<RetailerId>,
    /// `exact` refuses to pair two products that do not share a product code.
    /// `normalised` also pairs on brand, name and size, and marks what it
    /// guessed at.
    pub r#match: MatchMode,
}

impl Default for Compare {
    fn default() -> Compare {
        Compare {
            retailers: BANNERS.to_vec(),
            r#match: MatchMode::Normalised,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchMode {
    Exact,
    #[default]
    Normalised,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Auth {
    /// A shell command that prints the password on stdout, for a password
    /// manager. Beats the stored one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_command: Option<String>,
    /// Whether `auth login` keeps the password, so a lapsed session past
    /// renewal can be signed back in without a prompt.
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Retailer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
    /// A shell command that prints a bearer token, skipping the Club Plus
    /// handshake.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_command: Option<String>,
}

impl Config {
    pub fn load(file: &Path) -> AppResult<Config> {
        Ok(net_kit::config::load_toml(file)?)
    }

    pub fn retailer(&self, id: RetailerId) -> &Retailer {
        match id {
            RetailerId::NewWorld => &self.newworld,
            RetailerId::PaknSave => &self.paknsave,
            RetailerId::Woolworths => unreachable!("fsnz has no Woolworths"),
        }
    }

    pub fn retailer_mut(&mut self, id: RetailerId) -> &mut Retailer {
        match id {
            RetailerId::NewWorld => &mut self.newworld,
            RetailerId::PaknSave => &mut self.paknsave,
            RetailerId::Woolworths => unreachable!("fsnz has no Woolworths"),
        }
    }
}

/// Every setting, as a dotted key.
///
/// The list is explicit rather than derived so that `config list` has an order
/// worth reading, and so a key that is no longer real fails loudly instead of
/// writing a field nothing reads.
pub const KEYS: [&str; 10] = [
    "banner",
    "compare.retailers",
    "compare.match",
    "auth.password_command",
    "auth.store_password",
    "output.color",
    "newworld.store_id",
    "newworld.token_command",
    "paknsave.store_id",
    "paknsave.token_command",
];

/// What a key means, for `config list`.
pub fn describe(key: &str) -> &'static str {
    match key {
        "banner" => "the banner a command talks to when -b is not given",
        "compare.retailers" => "the banners `compare` puts side by side",
        "compare.match" => {
            "exact pairs only on product code; normalised also pairs on name and size"
        }
        "auth.password_command" => "a command that prints the password, for a password manager",
        "auth.store_password" => {
            "keep the password at login, so a lapsed session can renew unattended"
        }
        "output.color" => "auto, always or never",
        _ if key.ends_with("store_id") => {
            "the store prices are quoted against; `store set` resolves a name"
        }
        _ => "a command that prints a bearer token",
    }
}

impl Config {
    fn shop(key: &str) -> Option<(RetailerId, &str)> {
        let (shop, field) = key.split_once('.')?;
        let id = BANNERS.into_iter().find(|r| r.id() == shop)?;
        Some((id, field))
    }

    /// The value as it would be written, or `None` when nothing is set.
    pub fn get(&self, key: &str) -> AppResult<Option<String>> {
        if !KEYS.contains(&key) {
            return Err(unknown(key));
        }
        Ok(match key {
            "banner" => self.retailer.map(|r| r.id().to_string()),
            "compare.retailers" => Some(
                self.compare
                    .retailers
                    .iter()
                    .map(|r| r.id())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            "compare.match" => Some(
                match self.compare.r#match {
                    MatchMode::Exact => "exact",
                    MatchMode::Normalised => "normalised",
                }
                .into(),
            ),
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
            _ => {
                let (id, field) = Config::shop(key).ok_or_else(|| unknown(key))?;
                let shop = self.retailer(id);
                match field {
                    "store_id" => shop.store_id.clone(),
                    _ => shop.token_command.clone(),
                }
            }
        })
    }

    /// Parse and store a value, so a bad one is refused now rather than at the
    /// next command that reads it.
    pub fn set(&mut self, key: &str, value: &str) -> AppResult<()> {
        let value = value.trim();
        match key {
            "banner" => self.retailer = Some(parse(value)?),
            "compare.retailers" => {
                let shops: Vec<RetailerId> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(parse)
                    .collect::<AppResult<_>>()?;
                if shops.len() < 2 {
                    return Err(AppError::usage(
                        "comparing needs both banners, e.g. `nw,pns`",
                    ));
                }
                self.compare.retailers = shops;
            }
            "compare.match" => {
                self.compare.r#match = match value.to_lowercase().as_str() {
                    "exact" => MatchMode::Exact,
                    "normalised" | "normalized" | "fuzzy" => MatchMode::Normalised,
                    _ => return Err(AppError::usage("match takes `exact` or `normalised`")),
                }
            }
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
            _ => {
                let (id, field) = Config::shop(key).ok_or_else(|| unknown(key))?;
                match field {
                    "store_id" => self.retailer_mut(id).store_id = Some(value.to_string()),
                    "token_command" => {
                        self.retailer_mut(id).token_command = Some(value.to_string())
                    }
                    _ => return Err(unknown(key)),
                }
            }
        }
        Ok(())
    }

    /// Back to the default. Not the same as setting an empty string: an empty
    /// `password_command` would be run and would fail.
    pub fn unset(&mut self, key: &str) -> AppResult<()> {
        match key {
            "banner" => self.retailer = None,
            "compare.retailers" => self.compare.retailers = Compare::default().retailers,
            "compare.match" => self.compare.r#match = MatchMode::default(),
            "auth.password_command" => self.auth.password_command = None,
            "auth.store_password" => self.auth.store_password = Auth::default().store_password,
            "output.color" => self.output.color = ColorChoice::default(),
            _ => {
                let (id, field) = Config::shop(key).ok_or_else(|| unknown(key))?;
                match field {
                    "store_id" => self.retailer_mut(id).store_id = None,
                    "token_command" => self.retailer_mut(id).token_command = None,
                    _ => return Err(unknown(key)),
                }
            }
        }
        Ok(())
    }
}

fn parse(value: &str) -> AppResult<RetailerId> {
    let id: RetailerId = value.parse().map_err(|e| AppError::usage(format!("{e}")))?;
    if !BANNERS.contains(&id) {
        return Err(AppError::usage(format!(
            "fsnz is New World and PAK'nSAVE only, not {id}"
        )));
    }
    Ok(id)
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
        "no setting called {key:?}. Run `fsnz config list` for the {} there are.",
        KEYS.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_a_default_config() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.retailer, None);
        assert_eq!(cfg.compare.retailers, BANNERS.to_vec());
        assert!(cfg.auth.store_password);
    }

    #[test]
    fn a_saved_config_round_trips() {
        let mut cfg = Config {
            retailer: Some(RetailerId::PaknSave),
            ..Config::default()
        };
        cfg.retailer_mut(RetailerId::NewWorld).store_id = Some("s1".into());
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(text.contains("banner = \"paknsave\""), "{text}");
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.retailer, Some(RetailerId::PaknSave));
        assert_eq!(
            back.retailer(RetailerId::NewWorld).store_id.as_deref(),
            Some("s1")
        );
    }

    #[test]
    fn a_typo_in_a_key_is_reported_rather_than_ignored() {
        // `deny_unknown_fields` is the point: a config that silently does
        // nothing is worse than one that refuses to load.
        let err = toml::from_str::<Config>("baner = \"pns\"").unwrap_err();
        assert!(err.to_string().contains("baner"), "{err}");
    }

    #[test]
    fn woolworths_is_not_a_banner_this_tool_knows() {
        let mut cfg = Config::default();
        assert!(cfg.set("banner", "ww").is_err());
        assert!(cfg.set("banner", "nw").is_ok());
    }
}
