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
pub mod orders;
pub mod product;
pub mod settings;
pub mod stores;
pub mod update;
pub mod wishlist;

use kmart_api::Select;

use crate::app::App;
use crate::cli::Command;
use crate::error::AppResult;

pub async fn run(app: &App, command: Command) -> AppResult<()> {
    match command {
        Command::Search { query, listing } => listing::run(app, Select::Term(query), listing).await,
        Command::Browse { category, listing } => listing::browse(app, &category, listing).await,
        Command::Categories { query, depth } => categories::run(app, query.as_deref(), depth).await,
        Command::Product { keycode, no_stock } => product::show(app, &keycode, no_stock).await,
        Command::Stock { keycode, postcode } => {
            product::stock(app, &keycode, postcode.as_deref()).await
        }
        Command::Stores {
            query,
            postcode,
            limit,
        } => stores::list(app, query.as_deref(), postcode.as_deref(), limit).await,
        Command::Postcode { action } => settings::postcode(app, action).await,
        Command::Island { action } => settings::island(app, action),
        Command::Use { country } => settings::use_country(app, country),
        Command::Cart { action } => cart::run(app, action).await,
        Command::Wishlist { action } => wishlist::run(app, action).await,
        Command::Orders { limit } => orders::run(app, limit).await,
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
