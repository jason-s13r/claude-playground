//! `doctor` -- what is set up, and whether it works.
//!
//! Two halves. The header is what the tool decided before talking to anyone:
//! where its files are, which store is selected. Then the site itself, ending
//! in a live call, because "configured" and "working" are different claims and
//! only the second is worth much.

use cli_kit::{emit, human_duration, Out, View};
use gsnz_core::{AuthStatus, Caps, Fact};
use serde::Serialize;
use std::io::Write;
use std::time::Duration;

use crate::app::App;
use crate::error::AppResult;

/// One row of the capability list: the command a person would run, and how to
/// ask whether it can be.
type Feature = (&'static str, fn(&Caps) -> bool);

/// Named as the commands they gate, so a "no" reads as an answer rather than
/// as jargon.
const FEATURES: [Feature; 5] = [
    ("departments", |c| c.departments),
    ("orders show", |c| c.order_detail),
    ("orders previous", |c| c.previous_purchases),
    ("auth refresh", |c| c.refresh_session),
    ("auth import", |c| c.import_cookies),
];

pub async fn run(app: &App) -> AppResult<()> {
    let shop = examine(app).await;

    let report = Doctor {
        // Named, because a report gets pasted into a bug and "0.1.0" alone
        // does not say what of.
        version: format!("wwnz {}", crate::build::short_version()),
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
        secrets: net_kit::Backend::detect().describe().to_string(),
        shop,
    };
    let healthy = report.shop.healthy;
    emit(&mut app.out(), &report)?;
    // The report already said what failed and why, so this only carries the
    // code -- `doctor` in a script should not need its output parsed.
    if healthy {
        Ok(())
    } else {
        Err(crate::error::AppError::Reported(1))
    }
}

async fn examine(app: &App) -> Shop {
    let store_id = app.config.store_id.clone();
    let handle = match app.handle() {
        Ok(handle) => handle,
        Err(e) => {
            return Shop {
                facts: Vec::new(),
                store: Some(format!("cannot be set up: {e}")),
                login: None,
                api: Err(e.to_string()),
                caps: Caps::default(),
                healthy: false,
            }
        }
    };

    let auth = handle.auth_status().await.ok();
    // One cheap call that needs no account and no store, so it says whether
    // the chain works even for someone who has never signed in.
    let stores = handle.stores(None, u32::MAX).await;
    let api = match &stores {
        Ok(found) => Ok(format!("{} stores returned", found.len())),
        Err(e) => Err(e.to_string()),
    };
    // The store list is already in hand, so naming the selected one costs
    // nothing extra.
    let store = store_id.map(|id| {
        match stores
            .as_ref()
            .ok()
            .and_then(|all| all.iter().find(|s| s.id == id))
        {
            Some(store) => format!("{id} ({})", store.name),
            None => id,
        }
    });

    Shop {
        facts: handle.facts(),
        store,
        healthy: api.is_ok(),
        api,
        login: auth,
        caps: handle.caps(),
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
    facts: Vec<Fact>,
    store: Option<String>,
    login: Option<AuthStatus>,
    api: Result<String, String>,
    caps: Caps,
    /// Reachability only. Nobody having signed in is not a fault: most of this
    /// tool works signed out.
    healthy: bool,
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
        writeln!(out, "{}", out.heading("Woolworths"))?;
        for fact in &self.shop.facts {
            indented(out, fact.label, &fact.value)?;
        }
        if let Some(store) = &self.shop.store {
            indented(out, "store", store)?;
        } else {
            indented(out, "store", &out.warn("none selected"))?;
        }
        indented(out, "login", &describe_login(out, self.shop.login.as_ref()))?;
        // Not "api": that label is already a hostname above, and the same word
        // for a setting and for a result reads as a contradiction when one says
        // a URL and the other says "ok".
        match &self.shop.api {
            Ok(detail) => indented(out, "reachable", &format!("{}, {detail}", out.good("yes")))?,
            Err(e) => indented(out, "reachable", &format!("{}, {e}", out.bad("no")))?,
        }

        gaps(out, &self.shop.caps)?;

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

fn describe_login(out: &Out, status: Option<&AuthStatus>) -> String {
    let Some(status) = status else {
        return out.bad("could not be read").to_string();
    };
    if !status.signed_in {
        return out.dim("signed out").to_string();
    }
    let who = status.account.as_deref().unwrap_or("signed in");
    match (status.expires_in, &status.detail) {
        (Some(secs), _) => format!(
            "{who}, expires in {}",
            human_duration(Duration::from_secs(secs))
        ),
        (None, Some(detail)) => format!("{who}, {detail}"),
        (None, None) => who.to_string(),
    }
}

/// Only the commands this site cannot do.
///
/// Every line reading "yes" is a list that says nothing, so with no gaps there
/// is nothing to print -- and it comes back on its own the day something stops
/// being available.
fn gaps(out: &mut Out, caps: &Caps) -> std::io::Result<()> {
    let missing: Vec<&str> = FEATURES
        .iter()
        .filter(|(_, has)| !has(caps))
        .map(|(name, _)| *name)
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    writeln!(out)?;
    writeln!(out, "{}", out.heading("Not available"))?;
    for name in missing {
        indented(out, name, &out.dim("no"))?;
    }
    Ok(())
}
