//! `stores` and `store` -- finding a shop, and choosing one.

use cli_kit::emit;

use crate::app::App;
use crate::cli::StoreAction;
use crate::error::{AppError, AppResult};
use crate::views::{StoreList, StoreView};

pub async fn list(
    app: &App,
    query: Option<String>,
    region: Option<String>,
    refresh: bool,
    limit: u32,
) -> AppResult<()> {
    let client = std::sync::Arc::new(app.client()?);

    // A name to search for means the whole country: the finder is per region,
    // but somebody typing "whangarei" is asking where it is, not confirming
    // they already know. That is what the cached directory is for. Without a
    // name there is nothing to narrow two hundred stores by, so one region it
    // is.
    let (stores, scope) = match (&query, &region) {
        (Some(_), None) => (
            crate::directory::all(&client, &app.paths, refresh).await?,
            None,
        ),
        _ => {
            let region = region
                .or_else(|| app.config.region.clone())
                .unwrap_or_else(|| crate::commands::region::DEFAULT.to_string());
            let code = twlnz_api::region(&region)
                .ok_or_else(|| twlnz_api::Error::NoSuchStore(region.clone()))?;
            (client.stores(code).await?, Some(code))
        }
    };

    let mut stores = stores;
    if let Some(needle) = &query {
        stores.retain(|s| matches(s, needle));
    }
    stores.truncate(limit as usize);

    emit(&mut app.out(), &StoreList::new(&stores, scope))?;
    Ok(())
}

/// Whether a store answers to a name someone typed -- its own, its suburb or
/// its street.
fn matches(store: &twlnz_api::Store, needle: &str) -> bool {
    let needle = needle.to_lowercase();
    store.name.to_lowercase().contains(&needle)
        || store
            .city
            .as_deref()
            .is_some_and(|c| c.to_lowercase().contains(&needle))
        || store
            .address
            .as_deref()
            .is_some_and(|a| a.to_lowercase().contains(&needle))
}

pub async fn store(app: &App, action: StoreAction) -> AppResult<()> {
    let mut config = app.config.clone();
    match action {
        StoreAction::Show => {
            // Named where it can be. The id alone is not something anyone
            // recognises, and the region it is in is already known -- so this
            // is one request, and falling back to the bare id when it fails
            // keeps `store show` working offline.
            // Named off the cached directory, so this costs nothing once one
            // has been fetched and still works with no network. The bare id is
            // the fallback, because losing the name is not a reason to lose the
            // answer.
            let name = config.store_id.as_deref().and_then(|id| {
                crate::directory::cached(&app.paths)
                    .into_iter()
                    .flatten()
                    .find(|s| s.id == id)
                    .map(|s| s.name)
            });
            emit(
                &mut app.out(),
                &StoreView {
                    store_id: config.store_id.clone(),
                    name,
                },
            )?;
        }
        StoreAction::Set { store, region } => {
            let client = std::sync::Arc::new(app.client()?);
            let chosen = match &region {
                // An explicit region is an instruction about where to look, so
                // it is one request and no directory.
                Some(text) => {
                    let code = twlnz_api::region(text)
                        .ok_or_else(|| twlnz_api::Error::NoSuchStore(text.clone()))?;
                    pick(&client.stores(code).await?, &store)?.ok_or_else(|| {
                        twlnz_api::Error::NoSuchStore(format!(
                            "{store} in {code}. Leave `--region` off to look everywhere"
                        ))
                    })?
                }
                // Otherwise the whole country, off the cached directory: an id
                // read out of any listing should work without also saying which
                // region it came from.
                None => {
                    let all = crate::directory::all(&client, &app.paths, false).await?;
                    pick(&all, &store)?.ok_or_else(|| {
                        twlnz_api::Error::NoSuchStore(format!("{store} in any region"))
                    })?
                }
            };

            // Recorded locally, and deliberately not bound to the cart.
            // `Cart-SelectStore` is the server-side equivalent and it needs a
            // basket to bind to -- against an empty one it answers 500 with a
            // redirect to `/cart` -- so it belongs to checking out rather than
            // to setting a preference. Nothing this tool does needs the server
            // to agree: the store is here so `stock` knows which region to ask
            // about.
            config.store_id = Some(chosen.id.clone());
            // Kept alongside, so `stock` and `stores` default to where the
            // chosen store actually is rather than to Auckland.
            if let Some(region) = chosen.region.clone() {
                config.region = Some(region);
            }
            app.save(&config)?;
            emit(
                &mut app.out(),
                &StoreView {
                    store_id: Some(chosen.id),
                    name: Some(chosen.name),
                },
            )?;
        }
        StoreAction::Clear => {
            config.store_id = None;
            app.save(&config)?;
            emit(
                &mut app.out(),
                &StoreView {
                    store_id: None,
                    name: None,
                },
            )?;
        }
    }
    Ok(())
}

/// The one store a needle names, or an error if it names several.
///
/// An id matches outright; anything else has to name exactly one store, because
/// binding the cart to the wrong shop is not something a person would notice
/// until they turned up to collect.
fn pick(stores: &[twlnz_api::Store], needle: &str) -> AppResult<Option<twlnz_api::Store>> {
    if let Some(store) = stores.iter().find(|s| s.id == needle) {
        return Ok(Some(store.clone()));
    }
    let lowered = needle.to_lowercase();
    let matches: Vec<&twlnz_api::Store> = stores
        .iter()
        .filter(|s| s.name.to_lowercase().contains(&lowered))
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some((*one).clone())),
        many => Err(AppError::usage(format!(
            "{needle:?} matches {} stores: {}. Use the id.",
            many.len(),
            many.iter()
                .map(|s| format!("{} ({})", s.name, s.id))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}
