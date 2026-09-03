//! `stores` and `store` -- finding one, and choosing which one prices come
//! from.

use cli_kit::{emit, Out, View};
use gsnz_core::RetailerId;
use gsnz_ui::StoreList;
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::cli::StoreAction;
use crate::error::AppResult;

pub async fn list(app: &App, query: Option<String>, limit: u32) -> AppResult<()> {
    let handle = app.handle()?;
    let stores = handle.stores(query.as_deref(), limit).await?;
    emit(&mut app.out(), &StoreList(&stores))?;
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
    let shown = if app.selected.is_empty() {
        RetailerId::ALL.to_vec()
    } else {
        app.selected.clone()
    };
    let selection = Selection {
        stores: shown
            .into_iter()
            .map(|retailer| Selected {
                retailer,
                store_id: app.config.retailer(retailer).store_id.clone(),
            })
            .collect(),
    };
    emit(&mut app.out(), &selection)?;
    Ok(())
}

async fn set(app: &App, needle: &str) -> AppResult<()> {
    let retailer = app.retailer()?;
    let handle = app.handle()?;
    // Resolved against the live list rather than saved as typed: an id that
    // does not exist would otherwise fail on every later command instead of
    // this one.
    let store = handle.select_store(needle).await?;

    let mut app_config = app.config.clone();
    app_config.retailer_mut(retailer).store_id = Some(store.id.clone());
    if app_config.retailer.is_none() {
        // Choosing a store for a shop is as good a statement of "this is my
        // shop" as any, and saves a second command on first run.
        app_config.retailer = Some(retailer);
    }
    net_kit::config::save_toml(&app.config_file, &app_config)?;

    // Not the store *listing* view: its footer says "1 store. Select one:
    // gsnz store set ...", which after a successful `store set` reads as
    // though nothing happened.
    let selection = Selection {
        stores: vec![Selected {
            retailer,
            store_id: Some(store.id.clone()),
        }],
    };
    let mut out = app.out();
    if out.is_json() {
        emit(&mut out, &selection)?;
    } else {
        let where_it_is = store
            .where_it_is()
            .map(|w| format!(" ({w})"))
            .unwrap_or_default();
        writeln!(out, "{retailer}: {} {}{where_it_is}", store.id, store.name)?;
    }
    Ok(())
}

fn clear(app: &App) -> AppResult<()> {
    let retailer = app.retailer()?;
    let mut config = app.config.clone();
    let had = config.retailer_mut(retailer).store_id.take();
    net_kit::config::save_toml(&app.config_file, &config)?;
    let selection = Selection {
        stores: vec![Selected {
            retailer,
            store_id: None,
        }],
    };
    let mut out = app.out();
    if out.is_json() {
        emit(&mut out, &selection)?;
    } else {
        match had {
            Some(id) => writeln!(out, "{retailer}: forgot store {id}")?,
            None => writeln!(out, "{retailer}: no store was selected")?,
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct Selection {
    stores: Vec<Selected>,
}

#[derive(Serialize)]
struct Selected {
    retailer: RetailerId,
    store_id: Option<String>,
}

impl View for Selection {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        for s in &self.stores {
            match &s.store_id {
                Some(id) => writeln!(out, "{}  {id}", s.retailer)?,
                None => {
                    let hint = out.dim(&format!(
                        "(none: gsnz -b {} store set <name>)",
                        s.retailer.short()
                    ));
                    writeln!(out, "{}  {hint}", s.retailer)?;
                }
            }
        }
        Ok(())
    }
}
