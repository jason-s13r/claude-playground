//! `doctor` -- what is set up, and whether it works.
//!
//! The same two halves as every other tool here. The header is what this
//! program decided before talking to anyone: where its files are, which fascia
//! is the default. Then one section per fascia, ending in a live call, because
//! "configured" and "working" are different claims and only the second is
//! worth much.
//!
//! Reports **both** fascias whatever `-b` says, because the thing most likely
//! to confuse someone here is that they are separate accounts: being signed in
//! to Briscoes and not to Rebel Sport is an ordinary state, and a report that
//! showed one of them would make it look like a fault.

use bgnz_api::Banner;
use cli_kit::{emit, field, human_duration, indented, section, verdict, Out, View};
use net_kit::jwt;
use serde::Serialize;
use std::io::Write;
use std::time::Duration;

use crate::app::App;
use crate::error::{AppError, AppResult};

pub async fn run(app: &App) -> AppResult<()> {
    let mut fascias = Vec::new();
    for banner in Banner::ALL {
        fascias.push(examine(app, banner).await);
    }

    let report = Doctor {
        // Named, because a report gets pasted into a bug and "0.1.0" alone
        // does not say what of.
        version: format!("bgnz {}", crate::build::short_version()),
        config_file: format!(
            "{} ({})",
            app.config_file.display(),
            if app.config_file.exists() {
                "present"
            } else {
                "not written yet"
            }
        ),
        state_dir: app.paths.state_dir.display().to_string(),
        default: app.banner.name().to_string(),
        secrets: app.secrets().backend().describe().to_string(),
        // A warning rather than a fault: it is needed once per fascia to sign
        // in and never again, and there is a way in that skips it entirely.
        browser: crate::browser::available(app.env.browser_python.as_deref()),
        fascias,
    };

    let healthy = report.healthy();
    emit(&mut app.out(), &report)?;
    // The report already said which fascia failed and why, so this only
    // carries the code -- `doctor` in a script should not need its output
    // parsed.
    if healthy {
        Ok(())
    } else {
        Err(AppError::Reported(1))
    }
}

async fn examine(app: &App, banner: Banner) -> Fascia {
    let endpoints = app.endpoints_for(banner);
    // One read of the credential store, not two. The client loads the session
    // when it is built, so asking `App` for it again is a second trip through
    // the keychain -- and a second prompt, per fascia, on every run.
    let client = match app.client_for(banner) {
        Ok(client) => client,
        Err(e) => {
            return Fascia {
                name: banner.name().to_string(),
                storefront: endpoints.origin,
                search: Err(e.to_string()),
                store: None,
                login: Login::Error(e.to_string()),
                reachable: Err(e.to_string()),
                healthy: false,
            }
        }
    };

    // One cheap call that needs no account and no store, so it says whether
    // the chain works even for a fascia nobody has signed into.
    let search = match client.storefront().await {
        Ok(storefront) => Ok(storefront
            .klevu_key
            .unwrap_or_else(|| "no index published".into())),
        Err(e) => Err(e.to_string()),
    };
    let stores = match &search {
        Ok(_) => client.stores().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.clone()),
    };
    let reachable = stores
        .as_ref()
        .map(|s| format!("{} stores returned", s.len()))
        .map_err(Clone::clone);

    // The store list is already in hand, so naming the selected one costs
    // nothing extra.
    let store = app.config.store.get(banner).map(|id| {
        match stores
            .as_ref()
            .ok()
            .and_then(|all| all.iter().find(|s| s.id == id))
        {
            Some(store) => format!("{id} ({})", store.name),
            None => id.to_string(),
        }
    });

    let session = client.session();
    let login = if !session.can_refresh() {
        Login::SignedOut
    } else {
        // Spend the credential rather than reporting that one exists: a stored
        // sign-in that Gigya has since dropped looks identical from here until
        // it is used.
        match client.customer().await {
            Ok(customer) => Login::In {
                account: customer
                    .email
                    .or(session.email.clone())
                    .unwrap_or_else(|| "signed in".into()),
                expires_in: session
                    .expires_at
                    .and_then(|at| at.checked_sub(jwt::now_ms()))
                    .map(|left| left / 1000),
            },
            Err(e) => Login::Error(e.to_string()),
        }
    };

    Fascia {
        name: banner.name().to_string(),
        storefront: endpoints.origin,
        store,
        healthy: reachable.is_ok() && !matches!(login, Login::Error(_)),
        search,
        login,
        reachable,
    }
}

#[derive(Serialize)]
struct Doctor {
    version: String,
    config_file: String,
    state_dir: String,
    default: String,
    secrets: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    browser: Option<String>,
    fascias: Vec<Fascia>,
}

#[derive(Serialize)]
struct Fascia {
    name: String,
    storefront: String,
    search: Result<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    store: Option<String>,
    login: Login,
    reachable: Result<String, String>,
    healthy: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Login {
    SignedOut,
    In {
        account: String,
        expires_in: Option<u64>,
    },
    Error(String),
}

impl Doctor {
    /// Reachability and a credential that still works. A fascia nobody has
    /// signed into is not a fault: most of this tool works signed out.
    fn healthy(&self) -> bool {
        self.fascias.iter().all(|f| f.healthy)
    }
}

impl View for Doctor {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        writeln!(out, "{}", self.version)?;
        field(out, "config file", &self.config_file)?;
        field(out, "state dir", &self.state_dir)?;
        field(out, "default", &self.default)?;
        field(out, "secrets", &self.secrets)?;
        match &self.browser {
            Some(path) => field(out, "browser", path)?,
            None => field(
                out,
                "browser",
                &format!(
                    "{}; `bgnz auth token` needs none",
                    out.warn("camoufox not found")
                ),
            )?,
        }

        for fascia in &self.fascias {
            section(out, &fascia.name)?;
            indented(out, "storefront", &fascia.storefront)?;
            match &fascia.search {
                Ok(key) => indented(out, "search", key)?,
                Err(e) => indented(out, "search", &format!("{}, {e}", out.bad("no")))?,
            }
            match &fascia.store {
                Some(store) => indented(out, "store", store)?,
                None => indented(out, "store", &out.warn("none selected"))?,
            }
            indented(out, "login", &describe(out, &fascia.login))?;
            // Not "storefront": that label is already a hostname above, and
            // the same word for a setting and for a result reads as a
            // contradiction when one says a URL and the other says "ok".
            match &fascia.reachable {
                Ok(detail) => {
                    indented(out, "reachable", &format!("{}, {detail}", out.good("yes")))?
                }
                Err(e) => indented(out, "reachable", &format!("{}, {e}", out.bad("no")))?,
            }
        }

        verdict(out, self.healthy())
    }
}

fn describe(out: &Out, login: &Login) -> String {
    match login {
        Login::SignedOut => out.dim("signed out").to_string(),
        Login::Error(e) => format!("{}, {e}", out.bad("refused")),
        Login::In {
            account,
            expires_in,
        } => match expires_in {
            Some(secs) => format!(
                "{account}, token expires in {}",
                human_duration(Duration::from_secs(*secs))
            ),
            // Not a fault: the next command mints one from the stored sign-in
            // without asking anybody anything.
            None => format!("{account}, {}", out.dim("token will be minted")),
        },
    }
}
