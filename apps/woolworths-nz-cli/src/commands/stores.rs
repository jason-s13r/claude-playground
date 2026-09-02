//! `wwnz stores` and `wwnz store` -- finding a store, and choosing the one
//! prices are quoted against.

use anyhow::{bail, Result};

use crate::app::App;
use crate::cli::StoreCommand;
use crate::commands::io::print_json;
use crate::domain::Store;
use crate::output;

pub async fn list(app: &App, query: Option<&str>, limit: u32) -> Result<()> {
    let client = app.client().await?;
    // The API filters by name itself, which is what makes a search for a town
    // work at all -- there are far more stores than one page would hold.
    let stores = client.stores(query, limit).await?;

    if app.json {
        print_json(&serde_json::json!({
            "query": query,
            "count": stores.len(),
            "stores": stores,
        }));
        return Ok(());
    }

    if stores.is_empty() {
        match query {
            Some(q) => println!("No store matches '{q}'."),
            None => println!("No stores returned."),
        }
        return Ok(());
    }
    output::print_stores(&stores);
    Ok(())
}

pub async fn select(app: &mut App, cmd: &StoreCommand) -> Result<()> {
    match cmd {
        StoreCommand::Show => show(app).await,
        StoreCommand::Set { store } => set(app, store).await,
        StoreCommand::Clear => {
            app.config.store_id = None;
            app.config.store_name = None;
            app.config.save(&app.paths)?;
            if app.json {
                print_json(&serde_json::json!({ "store": null }));
            } else {
                println!("Store cleared.");
            }
            Ok(())
        }
    }
}

async fn show(app: &App) -> Result<()> {
    let Some(id) = app.config.store_id(app.store_flag.as_deref()) else {
        if app.json {
            print_json(&serde_json::json!({ "store": null }));
        } else {
            println!("No store selected. Set one: wwnz store set <name or town>");
        }
        return Ok(());
    };

    // The saved name is enough to report with, and avoids a round trip. Only
    // an id that arrived from --store or the environment needs looking up.
    let name = match app.config.store_name.clone() {
        Some(name) if app.config.store_id.as_deref() == Some(id.as_str()) => Some(name),
        _ => lookup(app, &id).await,
    };

    if app.json {
        print_json(&serde_json::json!({ "store": { "id": id, "name": name } }));
    } else {
        match name {
            Some(name) => println!("Pricing against {name} ({id})"),
            None => println!("Pricing against {id}"),
        }
    }
    Ok(())
}

/// Name a store id, best effort: an unreachable API is not a reason for
/// `store show` to fail, since the id alone is still the answer.
async fn lookup(app: &App, id: &str) -> Option<String> {
    let client = app.client().await.ok()?;
    let stores = client.stores(Some(id), 10).await.ok()?;
    stores.into_iter().find(|s| s.id == id).map(|s| s.name)
}

async fn set(app: &mut App, store: &str) -> Result<()> {
    let client = app.client().await?;
    let candidates = client.stores(Some(store), 50).await?;
    // The API's own matching is looser than what was asked for, so it is
    // narrowed again here before deciding whether the answer was unambiguous.
    let matches: Vec<Store> = candidates
        .into_iter()
        .filter(|s| s.matches(store))
        .collect();

    let chosen = match matches.len() {
        0 => bail!("no store matches '{store}'. List them: wwnz stores <town>"),
        1 => matches.into_iter().next().expect("length checked"),
        _ => match matches.iter().find(|s| s.id.eq_ignore_ascii_case(store)) {
            // An exact id among several name matches is unambiguous.
            Some(s) => s.clone(),
            None => {
                let names: Vec<String> = matches
                    .iter()
                    .map(|s| format!("  {}  {} ({})", s.id, s.name, s.where_it_is()))
                    .collect();
                bail!(
                    "'{store}' matches {} stores:\n{}\nUse an id.",
                    matches.len(),
                    names.join("\n")
                );
            }
        },
    };

    // Bind it now as well as saving it, so the choice is in effect for this
    // session and not only for the next command.
    let confirmed = client.set_store(&chosen.id).await?;

    app.config.store_id = Some(chosen.id.clone());
    app.config.store_name = Some(confirmed.clone().unwrap_or_else(|| chosen.name.clone()));
    app.config.save(&app.paths)?;

    if app.json {
        print_json(&serde_json::json!({
            "store": chosen,
            "config_file": app.paths.config_file(),
        }));
    } else {
        println!(
            "Pricing against {} ({}); saved to {}",
            chosen.name,
            chosen.id,
            app.paths.config_file().display()
        );
    }
    Ok(())
}
