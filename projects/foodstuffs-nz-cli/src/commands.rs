//! Command implementations, one module per subcommand group.

mod auth;
mod cart;
mod compare;
pub(crate) mod completions;
mod doctor;
mod io;
mod orders;
mod products;
mod stores;
mod update;

use anyhow::Result;
use std::process::ExitCode;

use crate::app::App;
use crate::banner::Banner;
use crate::cli::Command;

/// Run one command.
///
/// `auth status`, `doctor` and `update --check` report on a state rather than
/// changing one, so a bad state is not an error: they print their report and
/// exit non-zero.
pub async fn dispatch(app: &mut App, banner: Banner, command: &Command) -> Result<ExitCode> {
    let healthy = match command {
        Command::Search {
            query,
            list,
            specials,
        } => {
            products::list(app, banner, query, None, *specials, list).await?;
            true
        }
        Command::Specials { list } => {
            products::list(app, banner, "", None, true, list).await?;
            true
        }
        Command::Browse {
            department,
            list,
            specials,
        } => {
            products::list(app, banner, "", Some(department), *specials, list).await?;
            true
        }
        Command::Compare {
            query,
            list,
            specials,
        } => {
            compare::run(app, query, *specials, list).await?;
            true
        }
        Command::Stores { query } => {
            stores::list(app, banner, query.as_deref()).await?;
            true
        }
        Command::Store(cmd) => {
            stores::select(app, banner, cmd).await?;
            true
        }
        Command::Cart(cmd) => {
            cart::run(app, banner, cmd).await?;
            true
        }
        Command::Orders(cmd) => {
            orders::run(app, banner, cmd).await?;
            true
        }
        Command::Auth(cmd) => auth::run(app, cmd).await?,
        Command::Doctor => doctor::run(app).await?,
        // Handled in main, before there is an App to dispatch with.
        Command::Completions { .. } => true,
        Command::Update { check } => update::run(app, *check).await?,
    };

    Ok(if healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
