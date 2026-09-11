//! `stores` and `store`.
//!
//! The two ids a shop has are the trap here. `store_id` is what the
//! availability service takes; `fulfilment_number` is what `setStoreLocator`
//! takes. Sending one where the other belongs answers "no such store" *as a
//! success*, so this program settles on the store id everywhere a person types
//! one and converts at the boundary.

use cli_kit::emit;

use crate::app::App;
use crate::cli::StoreAction;
use crate::error::{AppError, AppResult};
use crate::views::{StoreList, StoreView};

pub async fn list(app: &App, query: Option<String>, collect: bool) -> AppResult<()> {
    let mut stores = app.client()?.stores().await?;
    if collect {
        stores.retain(|s| s.click_and_collect == Some(true));
    }
    if let Some(needle) = query.map(|q| q.to_lowercase()) {
        stores.retain(|s| {
            [
                Some(s.name.as_str()),
                s.city.as_deref(),
                s.region.as_deref(),
                s.display_name.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|field| field.to_lowercase().contains(&needle))
        });
    }
    stores.sort_by(|a, b| a.name.cmp(&b.name));

    let mut out = app.out();
    emit(&mut out, &StoreList::new(&stores, app.banner))?;
    Ok(())
}

pub async fn store(app: &App, action: StoreAction) -> AppResult<()> {
    match action {
        StoreAction::Show => show(app).await,
        StoreAction::Set { store } => set(app, store).await,
        StoreAction::Clear => {
            let mut config = app.config.clone();
            config.store.set(app.banner, None);
            app.save(&config)?;
            println!("Forgot the {} store.", app.banner);
            Ok(())
        }
    }
}

async fn show(app: &App) -> AppResult<()> {
    let chosen = app.config.store.get(app.banner);
    let mut view = StoreView {
        banner: app.banner.name().to_string(),
        store_id: chosen,
        name: None,
        address: None,
        hours: Vec::new(),
    };
    // Only worth a request when there is something to look up.
    if let Some(id) = chosen {
        if let Some(store) = app
            .client()?
            .stores()
            .await?
            .into_iter()
            .find(|s| s.id == id)
        {
            view.name = Some(store.name);
            view.address = store.address;
            view.hours = store.hours;
        }
    }
    let mut out = app.out();
    emit(&mut out, &view)?;
    Ok(())
}

async fn set(app: &App, store_id: i64) -> AppResult<()> {
    // Checked against the live list rather than taken on trust: a wrong id is
    // not rejected later, it answers "no such store" as a success, and finding
    // that out at the till is worse than one request now.
    let stores = app.client()?.stores().await?;
    let store = stores
        .iter()
        .find(|s| s.id == store_id)
        .ok_or_else(|| bgnz_api::Error::NoSuchStore(store_id.to_string()))?;

    let mut config = app.config.clone();
    config.store.set(app.banner, Some(store_id));
    app.save(&config)?;
    println!("{} store set to {} ({store_id}).", app.banner, store.name);

    // The storefront keeps its own copy against the account, so a signed-in
    // person sees the same shop on the website. Best effort: not being signed
    // in is normal and must not fail the local setting.
    if let Some(number) = &store.fulfilment_number {
        let client = app.client()?;
        match client.set_store(number).await {
            Ok(()) => println!("Also filed against the account."),
            Err(e) if e.is_lapsed() || e.needs_browser() => {}
            Err(e) => eprintln!("bgnz: could not file it against the account: {e}"),
        }
    }
    Ok(())
}

/// Turn a flag or config value into a store, or say what to do instead.
pub fn require(app: &App, flag: Option<i64>) -> AppResult<i64> {
    app.store(flag).ok_or_else(|| {
        AppError::usage(
            "no store chosen; pass --store <id>, or set one with `bgnz store set <id>` \
             (`bgnz stores` lists them)",
        )
    })
}
