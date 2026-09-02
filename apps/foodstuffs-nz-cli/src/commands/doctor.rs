//! `fsnz doctor` -- one pass over config, credentials and connectivity.

use anyhow::Result;
use std::time::Duration;

use crate::app::App;
use crate::auth;
use crate::banner::Banner;
use crate::build;
use crate::commands::io::{human_duration, print_json};
use crate::output;

/// Exits non-zero if anything is misconfigured or unreachable, so it can gate a
/// script.
pub async fn run(app: &App) -> Result<bool> {
    let mut healthy = true;
    let login = auth::load(&app.secrets).ok().flatten();
    let config_file = app.paths.config_file();
    let config_present = config_file.exists();
    let mut report = Vec::new();

    for banner in Banner::ALL {
        let endpoints = banner.endpoints();
        let mut entry = serde_json::Map::new();
        entry.insert("storefront".into(), endpoints.origin.clone().into());
        entry.insert("api".into(), endpoints.api.clone().into());

        let store_id = app.config.store_id(banner, app.store_flag.as_deref());
        entry.insert(
            "store_id".into(),
            store_id
                .clone()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        if store_id.is_none() {
            healthy = false;
        }

        match app.client(banner, false, false).await {
            Ok((_client, guest)) => {
                entry.insert("token".into(), guest.source.describe().into());
                entry.insert(
                    "token_expires_in_seconds".into(),
                    guest
                        .expires_in()
                        .map(|d| serde_json::Value::from(d.as_secs()))
                        .unwrap_or(serde_json::Value::Null),
                );
                match _client.stores().await {
                    Ok(stores) => {
                        entry.insert("api_reachable".into(), true.into());
                        entry.insert("stores_returned".into(), stores.len().into());
                        if let Some(id) = &store_id {
                            let name = stores.iter().find(|s| &s.id == id).map(|s| s.name.clone());
                            if name.is_none() {
                                healthy = false;
                            }
                            entry.insert(
                                "store_name".into(),
                                name.map(serde_json::Value::String)
                                    .unwrap_or(serde_json::Value::Null),
                            );
                        }
                    }
                    Err(e) => {
                        healthy = false;
                        entry.insert("api_reachable".into(), false.into());
                        entry.insert("error".into(), format!("{e:#}").into());
                    }
                }
            }
            Err(e) => {
                healthy = false;
                entry.insert("token".into(), serde_json::Value::Null);
                entry.insert("error".into(), format!("{e:#}").into());
            }
        }
        report.push((banner, entry));
    }

    if app.json {
        let banners: serde_json::Map<String, serde_json::Value> = report
            .iter()
            .map(|(b, e)| (b.id().to_string(), serde_json::Value::Object(e.clone())))
            .collect();
        print_json(&serde_json::json!({
            "version": crate::build::VERSION,
            "build": build::json(),
            "config_file": config_file,
            "config_present": config_present,
            "state_dir": app.paths.state_dir,
            "default_banner": app.config.default_banner().map(|b| b.id().to_string()).unwrap_or_default(),
            "logged_in_as": login.as_ref().map(|l| l.email.clone()),
            "credential_store": app.secrets.backend().describe(),
            "healthy": healthy,
            "banners": banners,
        }));
        return Ok(healthy);
    }

    println!("fsnz {}", build::short_version());
    println!(
        "config file  {} ({})",
        config_file.display(),
        if config_present { "present" } else { "missing" }
    );
    println!("state dir    {}", app.paths.state_dir.display());
    if let Ok(b) = app.config.default_banner() {
        println!("default      {}", b.name());
    }
    match &login {
        Some(l) => println!(
            "login        {} (in {})",
            l.email,
            app.secrets.backend().describe()
        ),
        None => println!("login        none; run `fsnz auth login`"),
    }

    for (banner, entry) in &report {
        println!("\n{}", banner.name());
        println!("  storefront   {}", str_field(entry, "storefront"));
        println!("  api          {}", str_field(entry, "api"));
        match entry.get("store_id").and_then(|v| v.as_str()) {
            Some(id) => {
                let name = entry.get("store_name").and_then(|v| v.as_str());
                match name {
                    Some(n) => println!("  store        {id} ({n})"),
                    None => println!("  store        {id}"),
                }
            }
            None => println!(
                "  store        not set; run `fsnz --banner {} stores <town>`",
                banner.id()
            ),
        }
        match entry.get("token").and_then(|v| v.as_str()) {
            Some(src) => {
                let secs = entry
                    .get("token_expires_in_seconds")
                    .and_then(|v| v.as_u64());
                match secs {
                    Some(s) => println!(
                        "  token        ok, {src}, expires in {}",
                        human_duration(Duration::from_secs(s))
                    ),
                    None => println!("  token        ok, {src}"),
                }
            }
            None => println!("  token        FAILED"),
        }
        match entry.get("api_reachable").and_then(|v| v.as_bool()) {
            Some(true) => {
                let n = entry
                    .get("stores_returned")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                println!(
                    "  api          ok, {n} store{} returned",
                    output::plural(n as usize)
                );
            }
            _ => println!("  api          FAILED"),
        }
        if let Some(err) = entry.get("error").and_then(|v| v.as_str()) {
            println!("  error        {err}");
        }
    }

    println!();
    println!("{}", if healthy { "healthy" } else { "unhealthy" });
    Ok(healthy)
}

fn str_field(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
    map.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}
