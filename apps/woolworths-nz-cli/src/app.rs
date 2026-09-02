//! The context every command runs against: resolved config, credentials, and
//! the HTTP client the API client is built from.

use anyhow::{Context, Result};

use crate::api::{Client, Endpoints};
use crate::config::{Config, Paths};
use crate::secrets::Secrets;
use crate::session::{self, Session, StoredSession};

pub struct App {
    pub secrets: Secrets,
    pub paths: Paths,
    pub config: Config,
    pub endpoints: Endpoints,
    pub http: wreq::Client,
    pub json: bool,
    pub store_flag: Option<String>,
}

impl App {
    /// A client for the guest-scoped endpoints: products, departments, stores.
    ///
    /// When there is a stored account session it is used anyway. Prices are the
    /// same either way, but a signed-in search is what the site itself does,
    /// and it keeps the cart's store and the search's store in step.
    pub async fn client(&self) -> Result<Client> {
        match self.stored_session()? {
            Some(session) => Ok(self.client_with(session)),
            None => Ok(self.client_with(self.guest_session().await?)),
        }
    }

    /// A client that must speak for an account: the cart, and order history.
    pub fn account_client(&self) -> Result<Client> {
        let session = self.stored_session()?.context(
            "this needs an account. Sign in first:\n  \
             wwnz auth login --email you@example.com",
        )?;
        Ok(self.client_with(session))
    }

    /// A client holding a guest token only, whatever is stored.
    pub async fn guest_client(&self) -> Result<Client> {
        Ok(self.client_with(self.guest_session().await?))
    }

    fn client_with(&self, session: Session) -> Client {
        Client::new(self.http.clone(), self.endpoints.clone(), session)
    }

    async fn guest_session(&self) -> Result<Session> {
        session::guest(&self.http, &self.endpoints.origin, &self.paths).await
    }

    /// The stored account session, if there is one.
    ///
    /// `WWNZ_SESSION` overrides it, holding a `Cookie` header value. That is
    /// the escape hatch for a session obtained some other way, and what the
    /// tests use.
    pub fn stored_session(&self) -> Result<Option<Session>> {
        if let Ok(raw) = std::env::var("WWNZ_SESSION") {
            if !raw.trim().is_empty() {
                let cookies = crate::auth::parse_cookie_header(&raw);
                return Ok(Some(Session::from_cookies(cookies)));
            }
        }
        Ok(StoredSession::load(&self.secrets)?.map(|s| s.session()))
    }
}
