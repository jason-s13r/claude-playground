//! `stores` -- what is near a postcode, or one store in full.

use cli_kit::emit;

use crate::app::App;
use crate::error::AppResult;
use crate::views::{StoreList, StoreView};

pub async fn list(
    app: &App,
    query: Option<&str>,
    postcode: Option<&str>,
    limit: u32,
) -> AppResult<()> {
    // A location id is looked up directly. It needs no postcode, which matters
    // because a store id is exactly what someone has when they are asking
    // about a shop somewhere they are not -- and it is the only way to see
    // trading hours, which the listing has no room for.
    if let Some(id) = query.filter(|q| is_location_id(q)) {
        let store = app.client().await?.store(id).await?;
        let mut out = app.out();
        return Ok(emit(&mut out, &StoreView { store: &store })?);
    }

    let postcode = app.postcode(postcode)?;
    let client = app.client().await?;
    let stores = client.stores_near(&postcode, limit as usize).await?;

    // Filtered here rather than in the request: the gateway offers no search,
    // only proximity, so this narrows what proximity returned.
    let stores: Vec<_> = match query {
        None => stores,
        Some(query) => {
            let wanted = query.trim().to_lowercase();
            stores
                .into_iter()
                .filter(|s| {
                    s.name.to_lowercase().contains(&wanted)
                        || s.city
                            .as_deref()
                            .is_some_and(|c| c.to_lowercase().contains(&wanted))
                })
                .collect()
        }
    };

    let mut out = app.out();
    Ok(emit(
        &mut out,
        &StoreList {
            stores: &stores,
            postcode: &postcode,
        },
    )?)
}

/// Whether this is a store id rather than a name to search for.
///
/// Kmart's are four digits. A shop named only in digits would be ambiguous,
/// and there is no such shop.
fn is_location_id(text: &str) -> bool {
    let text = text.trim();
    text.len() >= 3 && text.len() <= 6 && text.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_id_is_told_apart_from_a_name_to_search_for() {
        assert!(is_location_id("8229"));
        assert!(is_location_id("1384"));
        assert!(!is_location_id("Sylvia Park"));
        assert!(!is_location_id("Albany NZ"));
        // A postcode-length number is still an id here; `stores` takes
        // --postcode for the other meaning, so there is nothing to confuse.
        assert!(is_location_id("1010"));
        assert!(!is_location_id(""));
        assert!(!is_location_id("12345678"));
    }
}
