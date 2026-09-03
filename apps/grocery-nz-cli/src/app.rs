//! Everything a command needs, assembled once.
//!
//! The precedence rule is the same for every setting and is applied here so no
//! command has to remember it: **flag, then environment, then config file, then
//! the library default**. Below this file nothing consults any of the three.

use std::path::PathBuf;
use std::sync::Arc;

use cli_kit::{Format, Out};
use gsnz_core::{Error, Result, RetailerId};
use net_kit::{Backend, Paths, Secrets};

use crate::cli::Cli;
use crate::config::{ColorChoice, Config};
use crate::env::Overrides;
use crate::error::{AppError, AppResult};
use crate::retailers::{foodstuffs, woolworths, Handle, Registry};

/// The name the platform files this tool's config and state under. Its own,
/// not shared with `fsnz` or `wwnz`: three tools reading one another's tokens
/// would be a surprise the first time a logout took two of them down.
pub const APP: &str = "grocery-nz-cli";

pub struct App {
    pub config: Config,
    pub config_file: PathBuf,
    pub env: Overrides,
    pub registry: Registry,
    /// What `-b` said, if anything. Separate from the config default so
    /// `store set` can tell "this shop" from "the usual shop".
    pub selected: Option<RetailerId>,
    pub format: Format,
    pub color: bool,
}

impl App {
    pub fn new(cli: &Cli) -> AppResult<App> {
        let env = Overrides::read();
        let mut paths = Paths::defaults(APP)?;
        if let Some(dir) = &env.config_dir {
            paths = paths.with_config_dir(dir.clone());
        }
        if let Some(dir) = &env.state_dir {
            paths = paths.with_state_dir(dir.clone());
        }
        let config_file = paths.config_file();
        let config = Config::load(&config_file)?;

        let format = if cli.json { Format::Json } else { Format::Text };
        let color = match config.output.color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            // `NO_COLOR` is honoured whatever the config says: it is set by a
            // person's environment, which is more specific than a file they
            // wrote once.
            ColorChoice::Auto => !env.no_color,
        };

        let factory = Factory {
            env: env.clone(),
            config: config.clone(),
            paths: paths.clone(),
            backend: backend(env.secret_backend.as_deref()),
            store: cli.store.clone(),
            token: cli.token.clone(),
        };

        Ok(App {
            registry: Registry::new(move |id| factory.build(id)),
            selected: cli.retailer.or(config.retailer),
            config,
            config_file,
            env,
            format,
            color,
        })
    }

    pub fn out(&self) -> Out {
        Out::stdout(self.format, !self.color)
    }

    /// The one shop a per-retailer command talks to.
    pub fn retailer(&self) -> AppResult<RetailerId> {
        self.selected.ok_or_else(|| {
            AppError::usage(
                "no shop selected: pass `-b nw`, `-b pns` or `-b ww`, or set one for good \
                 with `gsnz -b <shop> store set <store>`",
            )
        })
    }

    pub fn handle(&self) -> AppResult<Handle> {
        Ok(self.registry.get(self.retailer()?)?)
    }
}

/// What builds an adapter. Cloned into the registry's closure so the registry
/// does not have to borrow the `App` that owns it.
#[derive(Clone)]
struct Factory {
    env: Overrides,
    config: Config,
    paths: Paths,
    backend: Backend,
    store: Option<String>,
    token: Option<String>,
}

impl Factory {
    fn build(&self, id: RetailerId) -> Result<Handle> {
        let family = family(id);
        // Config and state directories are shared; the credential *service* is
        // per family, because both halves file a password under the same
        // account name and one would otherwise overwrite the other.
        let secrets = Secrets::new(
            format!("{APP}.{family}"),
            self.backend,
            &self.paths.state_dir,
        );
        let paths = self.paths.scoped(family);
        let password = net_kit::password::Source::resolve(
            self.config.auth.password_command.as_deref(),
            &secrets,
        )
        .map_err(|e| Error::Other(e.to_string()))?;
        match id {
            RetailerId::NewWorld | RetailerId::PaknSave => {
                let banner = foodstuffs::convert::banner(id).expect("a Foodstuffs banner");
                let ov = self.retailer_env(id);
                let mut endpoints = fsnz_api::Endpoints::defaults(banner);
                if let Some(origin) = &ov.origin {
                    endpoints = endpoints.with_origin(origin.clone());
                }
                if let Some(api) = &ov.api {
                    endpoints = endpoints.with_api(api.clone());
                }
                let mut clubplus = fsnz_api::ClubPlusEndpoints::default();
                if let Some(origin) = &self.env.clubplus.origin {
                    clubplus = clubplus.with_login(origin.clone());
                }
                if let Some(api) = &self.env.clubplus.api {
                    clubplus = clubplus.with_api(api.clone());
                }
                Ok(Arc::new(foodstuffs::Foodstuffs::new(foodstuffs::Setup {
                    id,
                    endpoints,
                    clubplus,
                    paths,
                    secrets,
                    token_command: self.config.retailer(id).token_command.clone(),
                    explicit_token: self.token.clone().or_else(|| ov.token.clone()),
                    password,
                    store_id: self.store_id(id),
                })?))
            }
            RetailerId::Woolworths => {
                let ov = &self.env.woolworths;
                let mut endpoints = wwnz_api::Endpoints::default();
                if let Some(origin) = &ov.origin {
                    endpoints = endpoints.with_origin(origin.clone());
                }
                // Woolworths' second host is Auth0, not an API; `Retailer.api`
                // carries it because the shape is the same.
                if let Some(auth) = &ov.api {
                    endpoints = endpoints.with_auth(auth.clone());
                }
                Ok(Arc::new(woolworths::Woolworths::new(woolworths::Setup {
                    endpoints,
                    paths,
                    secrets,
                    password,
                    // The *flag* only. A Woolworths store is bound to the cart
                    // server-side, so a saved one is already in effect and a
                    // per-run override is a thing this shop cannot do -- which
                    // the adapter says outright rather than ignoring.
                    store_override: self.store.clone(),
                })?))
            }
        }
    }

    fn retailer_env(&self, id: RetailerId) -> &crate::env::Retailer {
        match id {
            RetailerId::NewWorld => &self.env.newworld,
            RetailerId::PaknSave => &self.env.paknsave,
            RetailerId::Woolworths => &self.env.woolworths,
        }
    }

    fn store_id(&self, id: RetailerId) -> Option<String> {
        self.store
            .clone()
            .or_else(|| self.retailer_env(id).store_id.clone())
            .or_else(|| self.config.retailer(id).store_id.clone())
    }
}

/// Which credential namespace a shop belongs to.
///
/// New World and PAK'nSAVE share one Club Plus login, so they share a
/// namespace; Woolworths has its own. [`RetailerId::catalogue`] already draws
/// exactly that line, for the same underlying reason.
pub fn family(id: RetailerId) -> &'static str {
    id.catalogue().unwrap_or_else(|| id.id())
}

fn backend(override_: Option<&str>) -> Backend {
    match override_.map(str::to_lowercase).as_deref() {
        Some("file") => Backend::File,
        Some("keyring") => Backend::Keyring,
        // Anything else, including nothing, means "whatever this machine has".
        _ => Backend::detect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_foodstuffs_banners_share_a_credential_namespace() {
        // One Club Plus login covers both, so filing them apart would mean
        // logging in twice for one account.
        assert_eq!(family(RetailerId::NewWorld), "foodstuffs");
        assert_eq!(family(RetailerId::PaknSave), "foodstuffs");
        assert_eq!(family(RetailerId::Woolworths), "woolworths");
    }

    #[test]
    fn an_unknown_backend_name_falls_back_rather_than_failing() {
        assert_eq!(backend(Some("file")), Backend::File);
        assert_eq!(backend(Some("keyring")), Backend::Keyring);
        assert_eq!(backend(Some("nonsense")), Backend::detect());
    }
}
