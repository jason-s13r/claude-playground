//! `doctor` -- what is set up, and whether it works.
//!
//! Two halves. The header is what the tool decided before talking to anyone:
//! where its files are, which country and postcode are selected. Then Kmart
//! itself, ending in live calls, because "configured" and "working" are
//! different claims and only the second is worth much.
//!
//! Two live calls rather than one, because this tool has two independent ways
//! to be broken. The catalogue needs no credentials; the gateway needs
//! imported cookies. Reporting them together would make a missing import look
//! like an outage.

use std::io::Write;
use std::time::Duration;

use cli_kit::{emit, human_duration, Out, View};
use serde::Serialize;

use crate::app::App;
use crate::error::{AppError, AppResult};

pub async fn run(app: &App) -> AppResult<()> {
    let shop = examine(app).await;
    let report = Doctor {
        // Named, because a report gets pasted into a bug and "0.1.0" alone
        // does not say what of.
        version: format!("kmart {}", crate::build::short_version()),
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
        secrets: app.backend().describe().to_string(),
        shop,
    };
    let healthy = report.shop.healthy;
    emit(&mut app.out(), &report)?;
    // The report already said what failed and why, so this only carries the
    // code -- `doctor` in a script should not need its output parsed.
    if healthy {
        Ok(())
    } else {
        Err(AppError::Reported(1))
    }
}

async fn examine(app: &App) -> Shop {
    let country = app.country;
    let client = match app.client().await {
        Ok(client) => client,
        Err(e) => {
            return Shop {
                country: format!("{} ({country})", country.name()),
                api: app.endpoints().api,
                postcode: app.config.postcode.clone(),
                island: app.island.clone(),
                login: None,
                admitted: false,
                catalogue: Err(e.to_string()),
                gateway: None,
                vendor: crate::vendor::Source::Baked,
                healthy: false,
            }
        }
    };

    // Reported rather than fetched again: `client()` has already resolved it,
    // and asking twice would make `doctor` say something no other command
    // does.
    let (_, vendor) = crate::vendor::resolve(
        &net_kit::http::build(kmart_api::client_spec()).unwrap_or_default(),
        &app.paths.state_dir,
        country,
        &app.endpoints().origin,
        |_| {},
    )
    .await;

    let session = client.session();
    let login = session.tokens().map(|t| Login {
        account: kmart_api::StoredSession::load(&app.secrets())
            .ok()
            .flatten()
            .and_then(|s| s.email),
        expires_in: Some(t.expires_at.saturating_sub(net_kit::jwt::now_secs())),
        renewable: t.refresh.is_some(),
    });

    // One cheap search that needs nothing at all, so it says whether the
    // catalogue works for someone who has never signed in or imported
    // anything. This is the half most people will only ever use.
    let catalogue = match client.search("mop", 1, 3).await {
        Ok(listing) if listing.products.is_empty() => {
            Err("a listing came back with no products in it".to_string())
        }
        Ok(listing) => Ok(format!("{} products parsed", listing.products.len())),
        Err(e) => Err(e.to_string()),
    };

    // And one against the gateway, which needs the imported cookies. Skipped
    // rather than failed when there are none: not having imported any is a
    // state, not a fault.
    let admitted = session.admitted(country);
    let gateway = match admitted {
        false => None,
        true => Some(
            match client
                .postcodes(app.config.postcode.as_deref().unwrap_or("1010"))
                .await
            {
                Ok(found) => Ok(format!("{} postcodes matched", found.len())),
                Err(e) => Err(e.to_string()),
            },
        ),
    };

    Shop {
        vendor,
        country: format!("{} ({country})", country.name()),
        api: app.endpoints().api,
        postcode: app.config.postcode.clone(),
        island: app.island.clone(),
        login,
        admitted,
        // Only the catalogue decides health. A missing cookie import is a
        // thing to do, not a thing that is broken, and a red `doctor` for it
        // would cry wolf on the machine of everyone who only ever searches.
        healthy: catalogue.is_ok() && gateway.as_ref().is_none_or(Result::is_ok),
        catalogue,
        gateway,
    }
}

#[derive(Serialize)]
struct Doctor {
    version: String,
    config_file: String,
    state_dir: String,
    secrets: String,
    shop: Shop,
}

#[derive(Serialize)]
struct Shop {
    country: String,
    api: String,
    postcode: Option<String>,
    island: Option<String>,
    login: Option<Login>,
    /// Whether the bot-check cookies are present for the country in use.
    admitted: bool,
    catalogue: Result<String, String>,
    /// `None` when there was nothing to try it with.
    gateway: Option<Result<String, String>>,
    /// Where the storefront's own identifiers came from this run.
    vendor: crate::vendor::Source,
    healthy: bool,
}

#[derive(Serialize)]
struct Login {
    account: Option<String>,
    expires_in: Option<u64>,
    renewable: bool,
}

/// The label column, wide enough for the longest label used.
const LABEL: usize = 14;

impl View for Doctor {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        writeln!(out, "{}", self.version)?;
        line(out, "config file", &self.config_file)?;
        line(out, "state dir", &self.state_dir)?;
        line(out, "secrets", &self.secrets)?;

        writeln!(out)?;
        writeln!(out, "{}", out.heading("Kmart"))?;
        indented(out, "country", &self.shop.country)?;
        indented(out, "gateway", &self.shop.api)?;
        indented(
            out,
            "postcode",
            &match &self.shop.postcode {
                Some(postcode) => postcode.clone(),
                // Worth a warning rather than a dim note: without one, `stock`
                // and `stores` cannot run at all.
                None => out.warn("not set, so stock and stores will not run"),
            },
        )?;
        indented(
            out,
            "island",
            &match &self.shop.island {
                Some(island) => island.clone(),
                None => out.dim("not set"),
            },
        )?;
        indented(out, "login", &describe_login(out, self.shop.login.as_ref()))?;
        indented(
            out,
            "cookies",
            &match self.shop.admitted {
                true => out.good("imported").to_string(),
                false => out.warn("none; run `kmart auth import <cookies.txt>`"),
            },
        )?;
        indented(
            out,
            "front end",
            &match self.shop.vendor {
                crate::vendor::Source::Fetched => {
                    format!("{}, read just now", out.good("current"))
                }
                crate::vendor::Source::Cached => {
                    format!("{}, read recently", out.good("current"))
                }
                // Not a fault: it is what every run did before the first
                // successful fetch, and everything works.
                crate::vendor::Source::Baked => out.dim("the values that shipped").to_string(),
            },
        )?;
        match &self.shop.catalogue {
            Ok(detail) => indented(out, "catalogue", &format!("{}, {detail}", out.good("ok")))?,
            Err(e) => indented(out, "catalogue", &format!("{}, {e}", out.bad("no")))?,
        }
        match &self.shop.gateway {
            None => indented(out, "gateway call", &out.dim("skipped, no cookies"))?,
            Some(Ok(detail)) => indented(
                out,
                "gateway call",
                &format!("{}, {detail}", out.good("ok")),
            )?,
            Some(Err(e)) => indented(out, "gateway call", &format!("{}, {e}", out.bad("no")))?,
        }

        writeln!(out)?;
        writeln!(
            out,
            "{}",
            if self.shop.healthy {
                out.good("healthy")
            } else {
                out.bad("not healthy")
            }
        )
    }
}

fn line(out: &mut Out, label: &str, value: &str) -> std::io::Result<()> {
    writeln!(out, "{label:<LABEL$} {value}")
}

fn indented(out: &mut Out, label: &str, value: &str) -> std::io::Result<()> {
    writeln!(out, "  {label:<width$} {value}", width = LABEL - 2)
}

fn describe_login(out: &Out, login: Option<&Login>) -> String {
    let Some(login) = login else {
        return out.dim("signed out").to_string();
    };
    let who = login.account.as_deref().unwrap_or("signed in");
    match login.expires_in {
        // Not a warning when it renews itself, which is the usual case here.
        Some(0) if login.renewable => format!("{who}, lapsed but renewable"),
        Some(0) => format!("{who}, {}", out.warn("expired")),
        Some(secs) => format!(
            "{who}, expires in {}",
            human_duration(Duration::from_secs(secs))
        ),
        None => who.to_string(),
    }
}

/// Kept next to the report because a country with no gateway host would make
/// every line above meaningless.
#[cfg(test)]
mod tests {
    use kmart_api::Country;

    #[test]
    fn every_country_has_somewhere_to_talk_to() {
        for country in Country::ALL {
            assert!(country.api().starts_with("https://"), "{country}");
            assert!(country.origin().starts_with("https://"), "{country}");
        }
    }
}
