//! One module per command. Each takes the assembled [`App`] and the flags it
//! was given, and nothing else: no command reads the environment, opens the
//! config file or decides how to render.

pub mod auth;
pub mod cart;
pub mod compare;
pub mod completions;
pub mod config;
pub mod departments;
pub mod doctor;
pub mod listing;
pub mod orders;
pub mod stores;
pub mod update;

use crate::app::App;
use crate::cli::Command;
use crate::error::AppResult;

pub async fn run(app: &App, command: Command) -> AppResult<()> {
    use gsnz_core::SearchBy;
    match command {
        Command::Search { query, listing } => {
            listing::run(app, SearchBy::Query(query), listing).await
        }
        Command::Specials { mut listing } => {
            // `specials` is `search` with the promotion filter pinned on, which
            // is why it takes no query.
            listing.specials = true;
            listing::run(app, SearchBy::Everything, listing).await
        }
        Command::Browse {
            department,
            listing,
        } => listing::run(app, SearchBy::Department(department), listing).await,
        // `store` was consumed by `Cli::store()` before the adapter was built.
        Command::Departments { query, depth, .. } => departments::run(app, query, depth).await,
        Command::Compare {
            query,
            listing,
            strict,
        } => compare::run(app, query, listing, strict).await,
        Command::Cart { action } => cart::run(app, action).await,
        Command::Orders { action } => orders::run(app, action).await,
        Command::Stores { query, limit } => stores::list(app, query, limit).await,
        Command::Store { action } => stores::store(app, action).await,
        Command::Use { banner } => config::use_shop(app, banner),
        Command::Config { action } => config::run(app, action),
        Command::Auth { action } => auth::run(app, action).await,
        Command::Doctor => doctor::run(app).await,
        Command::Update {
            version,
            check,
            pre_release,
        } => update::run(app, version, check, pre_release).await,
        Command::Completions { shell } => completions::run(app, shell),
    }
}
