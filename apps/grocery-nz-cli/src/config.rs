//! `~/.config/grocery-nz-cli/config.toml` -- the settings worth keeping.
//!
//! Everything here is optional and everything has a flag or an environment
//! variable that beats it. The file exists so that `gsnz store set` and
//! `gsnz -b ww` are remembered, not as a second way to configure the program.

use serde::{Deserialize, Serialize};
use std::path::Path;

use gsnz_core::RetailerId;

use crate::error::AppResult;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Which shop a bare command talks to. `gsnz -b ww` overrides it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retailer: Option<RetailerId>,
    pub compare: Compare,
    pub auth: Auth,
    pub output: Output,
    pub newworld: Retailer,
    pub paknsave: Retailer,
    pub woolworths: Retailer,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Compare {
    /// Which shops a bare `gsnz compare` spans.
    pub retailers: Vec<RetailerId>,
    /// `exact` refuses to pair two products that do not share a product code,
    /// which across catalogues means Woolworths never appears. `normalised`
    /// pairs on brand, name and size, and marks what it guessed at.
    pub r#match: MatchMode,
}

impl Default for Compare {
    fn default() -> Compare {
        Compare {
            retailers: RetailerId::ALL.to_vec(),
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Auth {
    /// A shell command that prints the password on stdout, for a password
    /// manager. Beats the stored one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_command: Option<String>,
    /// Whether `auth login` keeps the password, so a lapsed session can be
    /// renewed without a prompt. Woolworths sessions cannot be refreshed any
    /// other way.
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Retailer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
    /// A shell command that prints a bearer token. Foodstuffs only: a
    /// Woolworths guest token comes from loading a page, not from an API
    /// anything else could call.
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
            RetailerId::Woolworths => &self.woolworths,
        }
    }

    pub fn retailer_mut(&mut self, id: RetailerId) -> &mut Retailer {
        match id {
            RetailerId::NewWorld => &mut self.newworld,
            RetailerId::PaknSave => &mut self.paknsave,
            RetailerId::Woolworths => &mut self.woolworths,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_a_default_config() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.retailer, None);
        assert_eq!(cfg.compare.retailers, RetailerId::ALL.to_vec());
        assert!(cfg.auth.store_password);
    }

    #[test]
    fn a_saved_config_round_trips() {
        let mut cfg = Config {
            retailer: Some(RetailerId::Woolworths),
            ..Config::default()
        };
        cfg.retailer_mut(RetailerId::NewWorld).store_id = Some("s1".into());
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.retailer, Some(RetailerId::Woolworths));
        assert_eq!(
            back.retailer(RetailerId::NewWorld).store_id.as_deref(),
            Some("s1")
        );
    }

    #[test]
    fn a_typo_in_a_key_is_reported_rather_than_ignored() {
        // `deny_unknown_fields` is the point: a config that silently does
        // nothing is worse than one that refuses to load.
        let err = toml::from_str::<Config>("retialer = \"ww\"").unwrap_err();
        assert!(err.to_string().contains("retialer"), "{err}");
    }
}
