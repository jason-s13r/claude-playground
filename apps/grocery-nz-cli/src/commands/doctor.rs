//! `doctor` -- what is set up, and what each shop can do.
//!
//! The capability matrix is printed rather than discovered: a gap should be
//! something you are told about before you run into it.

use cli_kit::comfy_table::Cell;
use cli_kit::{emit, table, Check, Out, Report, View};
use gsnz_core::{Caps, RetailerId};
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::error::AppResult;

/// One row of the matrix: the command a person would run, and how to ask a
/// shop whether it can.
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
    let mut report = Report::new();

    report.push(Check::ok(
        "version",
        crate::build::short_version().to_string(),
    ));
    report.push(Check::ok(
        "config",
        format!("{}", app.config_file.display()),
    ));
    report.push(Check::ok(
        "state",
        format!("{}", app.paths.state_dir.display()),
    ));
    report.push(match net_kit::Backend::detect() {
        net_kit::Backend::Keyring => Check::ok("secrets", net_kit::Backend::Keyring.describe()),
        // Not a failure: a 0600 file is the documented fallback. But it is
        // worth knowing that tokens are on disk rather than in a keychain.
        net_kit::Backend::File => Check::warn("secrets", net_kit::Backend::File.describe())
            .with_hint("install a credential store, or set GSNZ_SECRET_BACKEND=file to silence"),
    });

    let mut caps = Vec::new();
    for id in RetailerId::ALL {
        let store = app.config.retailer(id).store_id.clone();
        report.push(match &store {
            Some(store) => Check::ok(format!("{id} store"), store.clone()),
            None => Check::warn(format!("{id} store"), "none selected")
                .with_hint(format!("gsnz -b {} store set <name>", id.short())),
        });
        match app.registry.get(id) {
            Ok(handle) => {
                report.push(match handle.auth_status().await {
                    Ok(s) if s.signed_in => Check::ok(
                        format!("{id} account"),
                        s.account.unwrap_or_else(|| "signed in".into()),
                    ),
                    // Signed out is normal: browsing needs no account.
                    Ok(_) => Check::skip(format!("{id} account"), "not signed in"),
                    Err(e) => Check::fail(format!("{id} account"), e.to_string()),
                });
                caps.push((id, handle.caps()));
            }
            Err(e) => {
                report.push(Check::fail(format!("{id} client"), e.to_string()));
            }
        }
    }

    emit(&mut app.out(), &Doctor { report, caps })?;
    Ok(())
}

#[derive(Serialize)]
struct Doctor {
    report: Report,
    caps: Vec<(RetailerId, Caps)>,
}

impl View for Doctor {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        self.report.text(out)?;
        if self.caps.is_empty() {
            return Ok(());
        }
        // Only when there is something to report. Every cell being "yes" is
        // a table that says nothing -- and the matrix exists to surface gaps,
        // so no gaps means no matrix. It comes back on its own the day a shop
        // cannot do something, which is the only day it is worth reading.
        let gaps: Vec<&Feature> = FEATURES
            .iter()
            .filter(|(_, has)| self.caps.iter().any(|(_, caps)| !has(caps)))
            .collect();
        if gaps.is_empty() {
            return Ok(());
        }
        writeln!(out)?;
        writeln!(out, "{}", out.heading("What some shops cannot do"))?;
        let mut headers: Vec<&str> = vec!["Command"];
        headers.extend(self.caps.iter().map(|(id, _)| id.name()));
        let mut t = table(&headers);
        for (name, has) in gaps {
            let mut cells = vec![Cell::new(name)];
            cells.extend(
                self.caps
                    .iter()
                    .map(|(_, caps)| Cell::new(if has(caps) { "yes" } else { "no" })),
            );
            t.add_row(cells);
        }
        writeln!(out, "{t}")
    }
}
