//! Command implementations, one module per subcommand group.

mod auth;
mod cart;
pub(crate) mod completions;
mod doctor;
mod io;
mod orders;
mod products;
mod stores;
mod update;

use anyhow::Result;
use std::process::ExitCode;

use crate::api::SearchBy;
use crate::app::App;
use crate::cli::Command;

/// Run one command.
///
/// `auth status`, `doctor` and `update --check` report on a state rather than
/// changing one, so a bad state is not an error: they print their report and
/// exit non-zero.
pub async fn dispatch(app: &mut App, command: &Command) -> Result<ExitCode> {
    let healthy = match command {
        Command::Search {
            query,
            list,
            specials,
        } => {
            products::list(app, SearchBy::Keyword(query.clone()), *specials, list).await?;
            true
        }
        Command::Specials { list } => {
            products::list(app, SearchBy::Specials, true, list).await?;
            true
        }
        Command::Browse {
            department,
            list,
            specials,
        } => {
            products::browse(app, department, *specials, list).await?;
            true
        }
        Command::Departments { query, depth } => {
            products::departments(app, query.as_deref(), *depth).await?;
            true
        }
        Command::Stores { query, limit } => {
            stores::list(app, query.as_deref(), *limit).await?;
            true
        }
        Command::Store(cmd) => {
            stores::select(app, cmd).await?;
            true
        }
        Command::Cart(cmd) => {
            cart::run(app, cmd).await?;
            true
        }
        Command::Orders(cmd) => {
            orders::run(app, cmd).await?;
            true
        }
        Command::Auth(cmd) => auth::run(app, cmd).await?,
        Command::Doctor => doctor::run(app).await?,
        // Handled in main, before there is an App to dispatch with.
        Command::Completions { .. } => true,
        Command::Update {
            version,
            check,
            pre_release,
        } => update::run(app, version.as_deref(), *check, *pre_release).await?,
    };

    Ok(if healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
