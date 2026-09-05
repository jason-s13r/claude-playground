//! `search` and `browse` -- the two ways to select products.
//!
//! One function, because they differ only in what goes into the query. Both
//! run against Constructor.io, which is the half of Kmart that needs no
//! credentials at all.

use cli_kit::emit;
use kmart_api::{Query, Select};

use crate::app::App;
use crate::cli::Listing;
use crate::error::{AppError, AppResult};
use crate::views::{FacetList, ProductList};

pub async fn run(app: &App, select: Select, flags: Listing) -> AppResult<()> {
    let client = app.client().await?;
    let mut query = Query::term("")
        .page(flags.page)
        .per_page(flags.limit)
        .island(app.country, app.island.as_deref());
    query.select = select;
    if let Some(sort) = &flags.sort {
        query = query.sort(sort);
    }
    for (name, value) in flags.filters().map_err(AppError::usage)? {
        query = query.filter(name, value);
    }

    let listing = client.listing(&query).await?;
    let mut out = app.out();

    // `--facets` asks what can be refined rather than for the products, so it
    // answers with that and stops. The listing was still fetched, because the
    // refinements are only ever reported alongside one.
    if flags.facets {
        return Ok(emit(
            &mut out,
            &FacetList {
                facets: &listing.facets,
            },
        )?);
    }

    Ok(emit(
        &mut out,
        &ProductList::new(&listing, app.country.name()).on(app.island.as_deref()),
    )?)
}

/// `browse`, which takes an id or enough of a name to be unambiguous.
///
/// A category id is a 32-character hash, which nobody types. So a name is
/// resolved against the tree first -- one extra request, and the difference
/// between a usable command and one that requires copying a hash out of
/// another command's output.
pub async fn browse(app: &App, category: &str, flags: Listing) -> AppResult<()> {
    let id = match is_group_id(category) {
        true => category.to_string(),
        false => resolve(app, category).await?,
    };
    run(app, Select::Group(id), flags).await
}

/// Whether this is already an id rather than something to look up.
fn is_group_id(text: &str) -> bool {
    text.len() == 32 && text.chars().all(|c| c.is_ascii_hexdigit())
}

async fn resolve(app: &App, query: &str) -> AppResult<String> {
    let client = app.client().await?;
    // Deep enough to reach the leaf categories people name -- "Mops" lives
    // three levels down -- without fetching the whole tree twice.
    let tree = client.categories(4).await?;
    let wanted = query.trim().to_lowercase();

    let all: Vec<&kmart_api::Category> = tree.iter().flat_map(|c| c.walk()).collect();
    // Exact name first, then a prefix, then anything containing it. Without
    // the ordering, "Men" would match "Womens Sleepwear" before "Men".
    let exact = all.iter().find(|c| c.name.to_lowercase() == wanted);
    let starts = all
        .iter()
        .find(|c| c.name.to_lowercase().starts_with(&wanted));
    let contains = all.iter().find(|c| {
        c.name.to_lowercase().contains(&wanted)
            || c.path
                .as_deref()
                .is_some_and(|p| p.to_lowercase().contains(&wanted))
    });

    exact
        .or(starts)
        .or(contains)
        .map(|c| c.id.clone())
        .ok_or_else(|| kmart_api::Error::NoSuchCategory(query.to_string()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_id_is_recognised_so_it_is_not_looked_up() {
        assert!(is_group_id("b947bb1ddc38b0ca89fe2fe20dbff54e"));
        // A name that happens to be long is not hex.
        assert!(!is_group_id("mops mop buckets and refills xxx"));
        assert!(!is_group_id("mops"));
        assert!(!is_group_id(""));
        // Right length, wrong alphabet.
        assert!(!is_group_id("z947bb1ddc38b0ca89fe2fe20dbff54e"));
    }
}
