//! `region` -- which of the sixteen the store finder looks in.
//!
//! Its own command, and named apart from [`crate::commands::island`] on
//! purpose. The site calls both of these "region": one is an
//! `islandAvailability` refinement that changes what a *listing contains*, the
//! other is an ISO `NZ-` code that decides which *shops* are asked about. They
//! are different questions with different answers, so they get different words
//! here even though the site does not bother.

use cli_kit::{emit, table, Out, View};
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::cli::RegionAction;
use crate::error::AppResult;

/// Where `stores` looks when nothing says otherwise.
pub const DEFAULT: &str = "NZ-AUK";

pub async fn run(app: &App, action: RegionAction) -> AppResult<()> {
    let mut config = app.config.clone();
    match action {
        RegionAction::List => {
            emit(
                &mut app.out(),
                &RegionList {
                    selected: config.region.clone(),
                },
            )?;
        }
        RegionAction::Set { region } => {
            // Resolved to a code as it is written, so no later command has to
            // work out what a name meant.
            let code = twlnz_api::region(&region)
                .ok_or_else(|| twlnz_api::Error::NoSuchStore(region.clone()))?;
            config.region = Some(code.to_string());
            app.save(&config)?;
            emit(&mut app.out(), &selected(Some(code)))?;
        }
        RegionAction::Clear => {
            config.region = None;
            app.save(&config)?;
            emit(&mut app.out(), &selected(None))?;
        }
        RegionAction::Show => {
            emit(&mut app.out(), &selected(config.region.as_deref()))?;
        }
    }
    Ok(())
}

/// `Northland (NZ-NTL)`, or the bare code for one this does not know.
pub fn label(code: &str) -> String {
    match name_of(code) {
        Some(name) => format!("{name} ({code})"),
        None => code.to_string(),
    }
}

fn selected(code: Option<&str>) -> RegionView {
    RegionView {
        name: code.and_then(name_of).map(str::to_string),
        region: code.map(str::to_string),
    }
}

/// The readable name for a code, so a report can say "Northland (NZ-NTL)"
/// rather than making the reader know the codes.
pub fn name_of(code: &str) -> Option<&'static str> {
    twlnz_api::REGIONS
        .iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(code))
        .map(|(_, name)| *name)
}

#[derive(Serialize)]
struct RegionView {
    region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl View for RegionView {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        match (&self.region, &self.name) {
            (Some(code), Some(name)) => {
                writeln!(out, "Stores and stock are looked up in {name} ({code}).")
            }
            (Some(code), None) => writeln!(out, "Stores and stock are looked up in {code}."),
            (None, _) => writeln!(
                out,
                "No region set, so stores and stock default to {} ({}). \
                 Run `twlnz region list` for the others.",
                name_of(DEFAULT).unwrap_or(DEFAULT),
                DEFAULT
            ),
        }
    }
}

#[derive(Serialize)]
struct RegionList {
    selected: Option<String>,
}

impl View for RegionList {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        let mut t = table(&["", "Code", "Region"]);
        for (code, name) in twlnz_api::REGIONS {
            let chosen = self
                .selected
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(code));
            t.add_row(vec![
                // Plain: a coloured cell is measured by its bytes and breaks
                // the column rules. See `views::stock_label`.
                if chosen { "*" } else { "" }.to_string(),
                code.to_string(),
                name.to_string(),
            ]);
        }
        writeln!(out, "{t}")?;
        match &self.selected {
            Some(_) => Ok(()),
            None => writeln!(
                out,
                "None selected; {} is the default. Set one: `twlnz region set <code>`.",
                DEFAULT
            ),
        }
    }

    fn json(&self) -> cli_kit::serde_json::Value {
        cli_kit::serde_json::json!({
            "selected": self.selected,
            "regions": twlnz_api::REGIONS
                .iter()
                .map(|(code, name)| cli_kit::serde_json::json!({ "code": code, "name": name }))
                .collect::<Vec<_>>(),
        })
    }
}
