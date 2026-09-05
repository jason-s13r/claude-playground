//! The three settings with commands of their own: the country, the postcode
//! and the island.
//!
//! Each is `kmart config set <key>` with a better name, because each is
//! something a person changes often enough that reaching for `config` would be
//! a nuisance.

use std::io::Write;

use kmart_api::Country;

use crate::app::App;
use crate::cli::{IslandAction, PostcodeAction};
use crate::error::AppResult;

/// `kmart use [au|nz]`.
pub fn use_country(app: &App, country: Option<Country>) -> AppResult<()> {
    let mut out = app.out();
    let Some(country) = country else {
        writeln!(out, "{} ({})", app.country.name(), app.country)?;
        return Ok(());
    };
    let mut config = app.config.clone();
    config.country = Some(country);
    app.save(&config)?;
    writeln!(out, "Now using {} ({country}).", country.name())?;
    // The catalogues are separate indexes with separate ids, so a keycode
    // noted in one country may not exist in the other.
    writeln!(
        out,
        "{}",
        out.dim("Prices, stock and the catalogue all change with it.")
    )?;
    Ok(())
}

/// `kmart postcode [show|find|set|clear]`.
pub async fn postcode(app: &App, action: Option<PostcodeAction>) -> AppResult<()> {
    let mut out = app.out();
    match action.unwrap_or(PostcodeAction::Show) {
        PostcodeAction::Show => match &app.config.postcode {
            Some(postcode) => writeln!(out, "{postcode}")?,
            None => writeln!(
                out,
                "No postcode set. Stock and stores need one: `kmart postcode set <POSTCODE>`."
            )?,
        },
        PostcodeAction::Find { query } => {
            let client = app.client().await?;
            let found = client.postcodes(&query).await?;
            if found.is_empty() {
                writeln!(out, "No postcode matching {query:?}.")?;
                return Ok(());
            }
            let mut t = cli_kit::table(&["Postcode", "Suburb", subdivision(app)]);
            for p in &found {
                t.add_row(vec![
                    p.postcode.clone(),
                    p.suburb.clone().unwrap_or_default(),
                    p.state.clone().unwrap_or_default(),
                ]);
            }
            writeln!(out, "{t}")?;
        }
        PostcodeAction::Set { postcode } => {
            let client = app.client().await?;
            // Checked before it is kept: a postcode Kmart does not know
            // answers every later stock question with nothing, which reads as
            // "out of stock everywhere" rather than as a bad setting.
            let found = client.postcodes(&postcode).await?;
            let confirmed = found.iter().find(|p| p.postcode == postcode.trim());
            let mut config = app.config.clone();
            config.postcode = Some(postcode.trim().to_string());

            // The island rides along, because the postcode already knows it
            // and asking a person for it separately would be asking twice.
            if app.country == Country::Nz {
                if let Some(state) = confirmed.and_then(|p| p.state.clone()) {
                    config.island = Some(state);
                }
            }
            app.save(&config)?;

            match confirmed {
                Some(p) => writeln!(
                    out,
                    "Postcode {} ({}).",
                    p.postcode,
                    p.suburb.clone().unwrap_or_default().trim()
                )?,
                None => {
                    writeln!(out, "Postcode {}.", postcode.trim())?;
                    writeln!(
                        out,
                        "{}",
                        out.dim("Kmart did not recognise it, so stock answers may be empty.")
                    )?;
                }
            }
        }
        PostcodeAction::Clear => {
            let mut config = app.config.clone();
            config.postcode = None;
            app.save(&config)?;
            writeln!(out, "Postcode forgotten.")?;
        }
    }
    Ok(())
}

/// `kmart island [show|set|clear]`.
pub fn island(app: &App, action: Option<IslandAction>) -> AppResult<()> {
    let mut out = app.out();
    // Not an error in Australia, just useless -- so it says so rather than
    // failing a command that would have worked.
    if app.country == Country::Au {
        writeln!(
            out,
            "{}",
            out.dim("Australia has no island filter; this setting only affects New Zealand.")
        )?;
    }
    match action.unwrap_or(IslandAction::Show) {
        IslandAction::Show => match &app.config.island {
            Some(island) => writeln!(out, "{island}")?,
            None => writeln!(out, "No island set; listings are not filtered.")?,
        },
        IslandAction::Set { island } => {
            let island = crate::config::parse_island(&island)?;
            let mut config = app.config.clone();
            config.island = Some(island.clone());
            app.save(&config)?;
            writeln!(out, "Listings filtered to the {island} island.")?;
        }
        IslandAction::Clear => {
            let mut config = app.config.clone();
            config.island = None;
            app.save(&config)?;
            writeln!(out, "Island forgotten.")?;
        }
    }
    Ok(())
}

/// `island` in New Zealand, `state` in Australia -- what the column is called.
fn subdivision(app: &App) -> &'static str {
    match app.country {
        Country::Au => "State",
        Country::Nz => "Island",
    }
}
