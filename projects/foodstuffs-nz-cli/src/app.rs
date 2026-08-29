//! The context every command runs against: resolved config, credentials, and
//! the HTTP client the API clients are built from.

use anyhow::{Context, Result};

use crate::api::Client;
use crate::banner::Banner;
use crate::config::{Config, Paths};
use crate::secrets::Secrets;
use crate::token::{self, GuestToken};

pub struct App {
    pub secrets: Secrets,
    pub paths: Paths,
    pub config: Config,
    pub http: reqwest::Client,
    pub json: bool,
    pub store_flag: Option<String>,
    pub token_flag: Option<String>,
}

impl App {
    /// The explicitly-supplied token for one banner, if any.
    ///
    /// `--token`/`FSNZ_TOKEN` names no banner, so it is only safe when the
    /// command talks to exactly one: the API rejects a New World token
    /// presented with a PAK'nSAVE store. `compare` and `doctor` touch both and
    /// must use the per-banner variables instead.
    fn explicit_token(&self, banner: Banner, single_banner: bool) -> Option<String> {
        if let Ok(v) = std::env::var(banner.token_env_key()) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
        if single_banner {
            self.token_flag.clone()
        } else {
            None
        }
    }

    /// Resolve a token and build a client for one banner.
    pub async fn client(
        &self,
        banner: Banner,
        force_refresh: bool,
        single_banner: bool,
    ) -> Result<(Client, GuestToken)> {
        self.client_inner(banner, force_refresh, single_banner, false)
            .await
    }

    /// A client holding an anonymous token, for the guest-scoped endpoints.
    pub async fn guest_client(&self, banner: Banner) -> Result<Client> {
        Ok(self.client_inner(banner, false, true, true).await?.0)
    }

    async fn client_inner(
        &self,
        banner: Banner,
        force_refresh: bool,
        single_banner: bool,
        guest: bool,
    ) -> Result<(Client, GuestToken)> {
        let endpoints = banner.endpoints();
        let guest = token::acquire(token::Request {
            banner,
            endpoints: &endpoints,
            paths: &self.paths,
            cfg: self.config.for_banner(banner),
            secrets: &self.secrets,
            explicit: self.explicit_token(banner, single_banner).as_deref(),
            force_refresh,
            guest,
        })
        .await?;
        let client = Client::new(self.http.clone(), banner, endpoints, guest.token.clone());
        Ok((client, guest))
    }

    pub fn store_id(&self, banner: Banner) -> Result<String> {
        self.config
            .store_id(banner, self.store_flag.as_deref())
            .filter(|s| !s.trim().is_empty())
            .with_context(|| {
                format!(
                    "no {} store selected. Prices and stock are per store:\n  \
                     fsnz --banner {} stores <town>\n  \
                     fsnz --banner {} store set <id or name fragment>",
                    banner.name(),
                    banner.id(),
                    banner.id(),
                )
            })
    }
}
