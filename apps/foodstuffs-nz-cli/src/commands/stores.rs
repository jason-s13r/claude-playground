//! `fsnz stores` and `fsnz store` -- finding a store, and choosing the one
//! prices are quoted against.

use anyhow::{bail, Result};

use crate::app::App;
use crate::banner::Banner;
use crate::cli::StoreCommand;
use crate::commands::io::print_json;
use crate::domain::Store;
use crate::output;

pub async fn list(app: &App, banner: Banner, query: Option<&str>) -> Result<()> {
    let (client, _) = app.client(banner, false, true).await?;
    let all = client.stores().await?;
    let stores = filter_stores(all, query);

    if app.json {
        print_json(&serde_json::json!({
            "banner": banner.id(),
            "count": stores.len(),
            "stores": stores,
        }));
        return Ok(());
    }

    if stores.is_empty() {
        match query {
            Some(q) => println!("No {} store matches '{q}'.", banner.name()),
            None => println!("{} returned no stores.", banner.name()),
        }
        return Ok(());
    }
    output::print_stores(&stores);
    Ok(())
}

fn filter_stores(stores: Vec<Store>, query: Option<&str>) -> Vec<Store> {
    match query.map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => stores.into_iter().filter(|s| s.matches(q)).collect(),
        None => stores,
    }
}

pub async fn select(app: &mut App, banner: Banner, cmd: &StoreCommand) -> Result<()> {
    match cmd {
        StoreCommand::Show => {
            let id = app.config.store_id(banner, app.store_flag.as_deref());
            let Some(id) = id else {
                if app.json {
                    print_json(&serde_json::json!({ "banner": banner.id(), "store": null }));
                } else {
                    println!(
                        "No {} store selected. Set one: fsnz --banner {} store set <name>",
                        banner.name(),
                        banner.id()
                    );
                }
                return Ok(());
            };

            // Name it if the API is reachable; the id alone is still useful.
            let named = match app.client(banner, false, true).await {
                Ok((client, _)) => client
                    .stores()
                    .await
                    .ok()
                    .and_then(|all| all.into_iter().find(|s| s.id == id)),
                Err(_) => None,
            };

            if app.json {
                print_json(&serde_json::json!({
                    "banner": banner.id(),
                    "store": named.clone().map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
                        .unwrap_or_else(|| serde_json::json!({ "id": id, "banner": banner.id() })),
                }));
            } else {
                match named {
                    Some(s) => println!("{}: {} ({})", banner.name(), s.name, s.id),
                    None => println!("{}: {id}", banner.name()),
                }
            }
            Ok(())
        }

        StoreCommand::Set { store } => {
            let (client, _) = app.client(banner, false, true).await?;
            let all = client.stores().await?;
            let matches: Vec<Store> = all.into_iter().filter(|s| s.matches(store)).collect();

            let chosen = match matches.len() {
                0 => bail!(
                    "no {} store matches '{store}'. List them: fsnz --banner {} stores",
                    banner.name(),
                    banner.id()
                ),
                1 => matches.into_iter().next().expect("length checked"),
                _ => {
                    // An exact id among several name matches is unambiguous.
                    match matches.iter().find(|s| s.id.eq_ignore_ascii_case(store)) {
                        Some(s) => s.clone(),
                        None => {
                            let names: Vec<String> = matches
                                .iter()
                                .map(|s| format!("  {}  {}", s.id, s.name))
                                .collect();
                            bail!(
                                "'{store}' matches {} {} stores:\n{}\nUse an id.",
                                matches.len(),
                                banner.name(),
                                names.join("\n")
                            );
                        }
                    }
                }
            };

            app.config.for_banner_mut(banner).store_id = Some(chosen.id.clone());
            app.config.save(&app.paths)?;

            if app.json {
                print_json(&serde_json::json!({
                    "banner": banner.id(),
                    "store": chosen,
                    "config_file": app.paths.config_file(),
                }));
            } else {
                println!(
                    "{}: pricing against {} ({}); saved to {}",
                    banner.name(),
                    chosen.name,
                    chosen.id,
                    app.paths.config_file().display()
                );
            }
            Ok(())
        }

        StoreCommand::Clear => {
            app.config.for_banner_mut(banner).store_id = None;
            app.config.save(&app.paths)?;
            if app.json {
                print_json(&serde_json::json!({ "banner": banner.id(), "store": null }));
            } else {
                println!("{}: store cleared.", banner.name());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_filter_matches_ids_and_name_fragments() {
        let stores = vec![
            Store {
                id: "a-1".into(),
                name: "New World Thorndon".into(),
                banner: "newworld",
                region: None,
                address: None,
            },
            Store {
                id: "b-2".into(),
                name: "New World Karori".into(),
                banner: "newworld",
                region: None,
                address: None,
            },
        ];
        assert_eq!(filter_stores(stores.clone(), Some("karori")).len(), 1);
        assert_eq!(filter_stores(stores.clone(), Some("a-1")).len(), 1);
        assert_eq!(filter_stores(stores.clone(), None).len(), 2);
        assert_eq!(filter_stores(stores, Some("new world")).len(), 2);
    }
}
