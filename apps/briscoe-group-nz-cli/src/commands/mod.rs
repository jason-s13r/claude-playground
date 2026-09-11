//! One module per command. Each takes the assembled [`App`] and the flags it
//! was given, and nothing else: no command reads the environment, opens the
//! config file or decides how to render.

pub mod auth;
pub mod cart;
pub mod categories;
pub mod completions;
pub mod config;
pub mod doctor;
pub mod listing;
pub mod loyalty;
pub mod orders;
pub mod product;
pub mod stores;
pub mod update;
pub mod wishlist;

use crate::app::App;
use crate::cli::Command;
use crate::error::AppResult;

pub async fn run(app: &App, command: Command) -> AppResult<()> {
    use bgnz_api::Query;
    match command {
        Command::Search { query, listing } => {
            listing::run(app, Query::search(query), listing).await
        }
        Command::Browse { category, listing } => {
            listing::run(app, Query::category(category), listing).await
        }
        Command::Categories { query, depth } => categories::run(app, query, depth).await,
        Command::Product { sku } => product::show(app, &sku).await,
        Command::Stock { sku, store } => product::stock(app, &sku, store).await,
        Command::Stores { query, collect } => stores::list(app, query, collect).await,
        Command::Store { action } => stores::store(app, action).await,
        Command::Cart { action } => cart::run(app, action).await,
        Command::Wishlist { action } => wishlist::run(app, action).await,
        Command::Orders { action } => orders::run(app, action).await,
        Command::Loyalty { refresh } => loyalty::run(app, refresh).await,
        Command::Auth { action } => auth::run(app, action).await,
        Command::Config { action } => config::run(app, action),
        Command::Doctor => doctor::run(app).await,
        Command::Update {
            version,
            check,
            pre_release,
        } => update::run(app, version, check, pre_release).await,
        Command::Completions { shell } => completions::run(app, shell),
    }
}
