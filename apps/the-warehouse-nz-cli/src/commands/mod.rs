//! One module per command. Each takes the assembled [`App`] and the flags it
//! was given, and nothing else: no command reads the environment, opens the
//! config file or decides how to render.

pub mod auth;
pub mod cart;
pub mod completions;
pub mod config;
pub mod departments;
pub mod doctor;
pub mod island;
pub mod listing;
pub mod product;
pub mod region;
pub mod stores;
pub mod update;
pub mod wishlist;

use crate::app::App;
use crate::cli::Command;
use crate::error::AppResult;

pub async fn run(app: &App, command: Command) -> AppResult<()> {
    use twlnz_api::Query;
    match command {
        Command::Search { query, listing } => {
            listing::run(app, Query::Keyword(query), listing).await
        }
        Command::Browse { category, listing } => {
            listing::run(app, Query::Category(category), listing).await
        }
        // `specials` is `browse` pinned to the clearance category, which is why
        // it takes no argument.
        Command::Specials { listing } => {
            listing::run(app, Query::Category(listing::SPECIALS.into()), listing).await
        }
        Command::Departments { query, depth } => departments::run(app, query, depth).await,
        Command::Product { pid, select } => product::show(app, &pid, &select).await,
        Command::Stock { pid, region } => product::stock(app, &pid, region.as_deref()).await,
        Command::Stores {
            query,
            region,
            refresh,
            limit,
        } => stores::list(app, query, region, refresh, limit).await,
        Command::Store { action } => stores::store(app, action).await,
        Command::Island { action } => island::run(app, action),
        Command::Region { action } => region::run(app, action).await,
        Command::Cart { action } => cart::run(app, action).await,
        Command::Wishlist { action } => wishlist::run(app, action).await,
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
