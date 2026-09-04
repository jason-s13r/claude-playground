//! `island` -- which island stock is quoted for.
//!
//! Its own command rather than a flag on every listing, because it changes what
//! a listing *contains*: The Warehouse ranges differently north and south, so a
//! product genuinely absent from one island's results is in stock on the other.
//!
//! Named for the thing itself, not for the site's `islandAvailability`
//! refinement, so that it cannot be mistaken for [`crate::commands::region`] --
//! those are the sixteen ISO `NZ-` codes and a different idea entirely. The
//! site calls both of them "region"; this does not.

use cli_kit::{emit, Out, View};
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::cli::IslandAction;
use crate::error::{AppError, AppResult};

pub fn run(app: &App, action: IslandAction) -> AppResult<()> {
    let mut config = app.config.clone();
    if matches!(action, IslandAction::List) {
        return emit(
            &mut app.out(),
            &IslandList {
                selected: config.island,
            },
        )
        .map_err(Into::into);
    }

    let island = match action {
        IslandAction::List => unreachable!("handled above"),
        IslandAction::Show => config.island,
        IslandAction::Set { island } => {
            let parsed = twlnz_api::Island::parse(&island).ok_or_else(|| {
                AppError::usage(format!(
                    "{island:?} is not an island; use `north` or `south`"
                ))
            })?;
            config.island = Some(parsed);
            app.save(&config)?;
            Some(parsed)
        }
        IslandAction::Clear => {
            config.island = None;
            app.save(&config)?;
            None
        }
    };

    emit(
        &mut app.out(),
        &IslandView {
            island: island.map(|i| i.to_string()),
        },
    )?;
    Ok(())
}

#[derive(Serialize)]
struct IslandList {
    selected: Option<twlnz_api::Island>,
}

impl View for IslandList {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        // Two rows and no table: a table around two words is furniture.
        for island in [twlnz_api::Island::North, twlnz_api::Island::South] {
            let mark = if self.selected == Some(island) {
                "*"
            } else {
                " "
            };
            writeln!(out, "{mark} {island}")?;
        }
        if self.selected.is_none() {
            writeln!(
                out,
                "{}",
                out.dim("None set; the site picks. Set one: `twlnz island set north`.")
            )?;
        }
        Ok(())
    }

    fn json(&self) -> cli_kit::serde_json::Value {
        cli_kit::serde_json::json!({
            "selected": self.selected.map(|i| i.to_string()),
            "islands": ["north", "south"],
        })
    }
}

#[derive(Serialize)]
struct IslandView {
    island: Option<String>,
}

impl View for IslandView {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        match &self.island {
            Some(island) => writeln!(out, "Listings are for the {island} island."),
            None => writeln!(
                out,
                "No island set, so listings show whatever the site defaults to. \
                 Run `twlnz island set north` or `twlnz island set south`."
            ),
        }
    }
}
