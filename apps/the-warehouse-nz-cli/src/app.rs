//! Everything a command needs, assembled once.
//!
//! The precedence rule is the same for every setting and is applied here so no
//! command has to remember it: **flag, then environment, then config file, then
//! the library default**. Below this file nothing consults any of the three.

use std::path::PathBuf;

use cli_kit::{Format, Out};
use net_kit::{Backend, Paths, Secrets};
use twlnz_api::{Client, Endpoints, Island, StoredSession};

use crate::cli::Cli;
use crate::config::{ColorChoice, Config};
use crate::env::Overrides;
use crate::error::{AppError, AppResult};

/// The name the platform files this tool's config and state under. Its own, not
/// shared with the grocery tools: two tools reading one another's tokens would
/// be a surprise the first time a logout took both down.
pub const APP: &str = "the-warehouse-nz-cli";

pub struct App {
    pub config: Config,
    pub config_file: PathBuf,
    pub paths: Paths,
    pub env: Overrides,
    pub format: Format,
    pub color: bool,
    /// The island this run uses: the flag if one was given, otherwise the
    /// config. Resolved here so no command re-derives it.
    pub island: Option<Island>,
}

impl App {
    pub fn new(cli: &Cli) -> AppResult<App> {
        let env = Overrides::get().clone();
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

        let island = match cli.island() {
            Some(text) => Some(Island::parse(text).ok_or_else(|| {
                AppError::usage(format!("{text:?} is not an island; use `north` or `south`"))
            })?),
            None => config.island,
        };

        Ok(App {
            config,
            config_file,
            paths,
            env,
            format,
            color,
            island,
        })
    }

    /// The credential store the account's secrets are filed in.
    pub fn secrets(&self) -> Secrets {
        Secrets::new(
            APP,
            backend(self.env.secret_backend.as_deref()),
            &self.paths.state_dir,
        )
    }

    pub fn out(&self) -> Out {
        Out::stdout(self.format, !self.color)
    }

    pub fn endpoints(&self) -> Endpoints {
        let endpoints = Endpoints::default();
        match &self.env.origin {
            Some(origin) => endpoints.with_origin(origin.clone()),
            None => endpoints,
        }
    }

    /// The client every command talks through.
    ///
    /// Built per call rather than cached: it is one HTTP client and a cookie
    /// map, no command needs two, and a lazy cell here would only exist to hide
    /// a cost that is not there.
    pub fn client(&self) -> AppResult<Client> {
        let http = net_kit::http::build(twlnz_api::client_spec())
            .map_err(|e| AppError::usage(format!("building the HTTP client: {e}")))?;
        let secrets = self.secrets();
        let stored = StoredSession::load(&secrets)?;
        let session = stored
            .as_ref()
            .map(StoredSession::session)
            .unwrap_or_default();

        // Only offered when there is both an email to sign in *as* and a
        // password to do it with. Without either, the client reports a lapsed
        // session rather than prompting from inside a call that was meant to
        // read a price.
        let password = net_kit::password::Source::resolve(
            self.config.auth.password_command.as_deref(),
            &secrets,
        )
        .ok()
        .flatten();
        let reauth = stored
            .and_then(|s| s.email)
            .zip(password)
            .map(|(email, password)| twlnz_api::Reauth {
                email,
                password,
                secrets: self.secrets(),
            });

        Ok(Client::new(http, self.endpoints(), session)
            .with_reauth(reauth)
            .with_island(self.island)
            .with_debug(self.env.debug))
    }

    /// Write the config back, having changed it.
    pub fn save(&self, config: &Config) -> AppResult<()> {
        config.save(&self.config_file)
    }
}

impl Cli {
    /// The `--island` this run was given, wherever it was given.
    ///
    /// Every listing takes one and the client is built before the command runs,
    /// so it has to be found from up here.
    pub fn island(&self) -> Option<&str> {
        use crate::cli::Command;
        match &self.command {
            Command::Search { listing, .. }
            | Command::Browse { listing, .. }
            | Command::Specials { listing } => listing.island.as_deref(),
            _ => None,
        }
    }
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
    fn an_unknown_backend_name_falls_back_rather_than_failing() {
        assert_eq!(backend(Some("file")), Backend::File);
        assert_eq!(backend(Some("keyring")), Backend::Keyring);
        assert_eq!(backend(Some("nonsense")), Backend::detect());
    }
}
