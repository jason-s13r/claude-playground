//! `stores` and `store` -- finding one, and choosing which one prices come
//! from.

use cli_kit::{emit, Out, View};
use gsnz_ui::StoreList;
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::cli::StoreAction;
use crate::error::AppResult;

pub async fn list(app: &App, query: Option<String>, limit: u32) -> AppResult<()> {
    let handle = app.handle()?;
    let stores = handle.stores(query.as_deref(), limit).await?;
    emit(
        &mut app.out(),
        &StoreList::new(&stores).next("Select one: wwnz store set <id or name fragment>"),
    )?;
    Ok(())
}

pub async fn store(app: &App, action: StoreAction) -> AppResult<()> {
    match action {
        StoreAction::Show => show(app),
        StoreAction::Set { store } => set(app, &store).await,
        StoreAction::Clear => clear(app),
    }
}

/// What is selected, without asking anyone.
///
/// Deliberately no network call: `store show` is what someone runs when a
/// command has just failed, and it should answer even when the API will not.
fn show(app: &App) -> AppResult<()> {
    emit(
        &mut app.out(),
        &Selected {
            store_id: app.config.store_id.clone(),
        },
    )?;
    Ok(())
}

/// Not a local preference: prices are quoted against whatever store the cart is
/// bound to, so this reaches the server. The id is written to the config file
/// as well, so a listing can be headed with it without a round trip.
async fn set(app: &App, needle: &str) -> AppResult<()> {
    let handle = app.handle()?;
    // Resolved against the live list rather than saved as typed: an id that
    // does not exist would otherwise fail on every later command instead of
    // this one.
    let store = handle.select_store(needle).await?;

    let mut config = app.config.clone();
    config.store_id = Some(store.id.clone());
    net_kit::config::save_toml(&app.config_file, &config)?;

    // Not the store *listing* view: its footer says "1 store. Select one:
    // wwnz store set ...", which after a successful `store set` reads as
    // though nothing happened.
    let mut out = app.out();
    if out.is_json() {
        emit(
            &mut out,
            &Selected {
                store_id: Some(store.id.clone()),
            },
        )?;
    } else {
        let where_it_is = store
            .where_it_is()
            .map(|w| format!(" ({w})"))
            .unwrap_or_default();
        writeln!(out, "{} {}{where_it_is}", store.id, store.name)?;
    }
    Ok(())
}

/// Forgets the local record only. The cart stays bound to whatever store it was
/// bound to, because nothing on this site unbinds it.
fn clear(app: &App) -> AppResult<()> {
    let mut config = app.config.clone();
    let had = config.store_id.take();
    net_kit::config::save_toml(&app.config_file, &config)?;
    let mut out = app.out();
    if out.is_json() {
        emit(&mut out, &Selected { store_id: None })?;
    } else {
        match had {
            Some(id) => writeln!(out, "forgot store {id}")?,
            None => writeln!(out, "no store was selected")?,
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct Selected {
    store_id: Option<String>,
}

impl View for Selected {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        match &self.store_id {
            Some(id) => writeln!(out, "{id}"),
            None => {
                let hint = out.dim("(none: wwnz store set <name>)");
                writeln!(out, "{hint}")
            }
        }
    }
}
