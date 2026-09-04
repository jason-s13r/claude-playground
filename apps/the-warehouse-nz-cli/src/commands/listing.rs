//! `search`, `browse` and `specials` -- one code path with three ways in.

use cli_kit::emit;
use twlnz_api::Query;

use crate::app::App;
use crate::cli::Listing;
use crate::error::AppResult;
use crate::views::ProductList;

/// The clearance category, which is what `specials` browses.
///
/// A category rather than a filter: unlike a grocery site, The Warehouse has no
/// "on promotion" flag across the catalogue -- reductions are a department.
pub const SPECIALS: &str = "specials";

pub async fn run(app: &App, query: Query, flags: Listing) -> AppResult<()> {
    let client = app.client()?;
    let facets = flags.facets();
    let listing = client
        .search(&query, flags.limit, flags.sort.as_deref(), &facets)
        .await?;

    emit(
        &mut app.out(),
        &ProductList::new(&listing.products)
            .of(listing.total)
            .in_category(listing.category.as_deref())
            .on(app.island),
    )?;
    Ok(())
}
