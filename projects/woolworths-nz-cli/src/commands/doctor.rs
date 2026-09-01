//! `wwnz doctor` -- check the setup without changing any of it.
//!
//! Each check prints a line and contributes to the exit status, so this can
//! gate a script. Nothing here writes config, and no check aborts the run: a
//! failure early on is exactly when the later lines are most worth seeing.

use anyhow::Result;

use crate::app::App;
use crate::commands::io::print_json;
use crate::domain::order::Filter;

pub async fn run(app: &App) -> Result<bool> {
    // What this tool is and where it keeps things. None of it can fail, so it
    // is stated rather than checked.
    let mut checks: Vec<Check> = vec![
        Check::ok("build", crate::build::short_version().to_string()),
        Check::ok("endpoint", app.endpoints.graphql()),
        Check::ok("config file", app.paths.config_file().display().to_string()),
        Check::ok("secrets", app.secrets.backend().describe().into()),
    ];

    match app.config.store_id(app.store_flag.as_deref()) {
        Some(id) => {
            let name = app.config.store_name.as_deref().unwrap_or("(unnamed)");
            checks.push(Check::ok("store", format!("{name} ({id})")));
        }
        None => checks.push(Check::warn(
            "store",
            "none selected; prices come from a default store. \
             Set one: wwnz store set <town>",
        )),
    }

    // Connectivity, via the one call that needs no account and no store.
    match app.guest_client().await {
        Ok(client) => match client.stores(Some("woolworths"), 1).await {
            Ok(stores) => checks.push(Check::ok(
                "guest access",
                format!("working ({} store(s) returned)", stores.len()),
            )),
            Err(e) => checks.push(Check::fail("guest access", format!("{e:#}"))),
        },
        Err(e) => checks.push(Check::fail("guest token", format!("{e:#}"))),
    }

    // The account, if there is one. A stored session that no longer works is
    // the single most useful thing this command can find.
    match app.stored_session()? {
        None => checks.push(Check::warn(
            "account",
            "not signed in; the cart and orders are unavailable. \
             Run: wwnz auth login",
        )),
        Some(_) => match app.account_client() {
            Ok(client) => match client.orders(1, Filter::All).await {
                Ok(page) => checks.push(Check::ok(
                    "account",
                    format!("signed in ({} order(s) on file)", page.total),
                )),
                Err(e) => checks.push(Check::fail(
                    "account",
                    format!("the stored session was refused: {e:#}"),
                )),
            },
            Err(e) => checks.push(Check::fail("account", format!("{e:#}"))),
        },
    }

    let healthy = checks.iter().all(|c| c.status != Status::Fail);

    if app.json {
        print_json(&serde_json::json!({
            "healthy": healthy,
            "build": crate::build::json(),
            "checks": checks.iter().map(Check::json).collect::<Vec<_>>(),
        }));
        return Ok(healthy);
    }

    for check in &checks {
        println!(
            "{} {:<14} {}",
            check.status.mark(),
            check.name,
            check.detail
        );
    }
    if !healthy {
        println!("\nSomething above is broken; the lines marked x say what.");
    }
    Ok(healthy)
}

#[derive(PartialEq, Eq)]
enum Status {
    Ok,
    Warn,
    Fail,
}

impl Status {
    fn mark(&self) -> &'static str {
        match self {
            Status::Ok => "ok  ",
            Status::Warn => "note",
            Status::Fail => "x   ",
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Fail => "fail",
        }
    }
}

struct Check {
    name: &'static str,
    status: Status,
    detail: String,
}

impl Check {
    fn ok(name: &'static str, detail: String) -> Check {
        Check {
            name,
            status: Status::Ok,
            detail,
        }
    }

    /// Something worth saying that is not a failure -- no store chosen, no
    /// account. Neither stops the tool working, so neither fails the run.
    fn warn(name: &'static str, detail: &str) -> Check {
        Check {
            name,
            status: Status::Warn,
            detail: detail.to_string(),
        }
    }

    fn fail(name: &'static str, detail: String) -> Check {
        Check {
            name,
            status: Status::Fail,
            detail,
        }
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "status": self.status.name(),
            "detail": self.detail,
        })
    }
}
