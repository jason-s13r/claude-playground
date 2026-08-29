//! Command implementations, one module per subcommand group.

mod auth;
mod cart;
mod compare;
mod doctor;
mod io;
mod orders;
mod products;
mod stores;

use anyhow::Result;
use std::process::ExitCode;

use crate::app::App;
use crate::banner::Banner;
use crate::cli::Command;

/// Run one command.
///
/// `auth status` and `doctor` report on a state rather than changing one, so a
/// bad state is not an error: they print their report and exit non-zero.
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
        Command::Auth(cmd) => auth::run(app, banner, cmd).await?,
        Command::Doctor => doctor::run(app).await?,
    };

    Ok(if healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
