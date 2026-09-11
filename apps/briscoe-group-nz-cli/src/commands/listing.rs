//! `search` and `browse`, which are one request to the search index.
//!
//! Both go to Klevu rather than to the storefront's GraphQL, because that is
//! what the website does: the product grid is Klevu's, so results here are the
//! ones the site would show, in the order it would show them. Magento can
//! answer a keyword search perfectly well, and ranks it differently -- a
//! listing built on it would quietly disagree with the site it mirrors.

use bgnz_api::Query;
use cli_kit::emit;

use crate::app::App;
use crate::cli::Listing;
use crate::error::{AppError, AppResult};
use crate::views::ProductList;

pub async fn run(app: &App, query: Query, flags: Listing) -> AppResult<()> {
    if let Some(sort) = &flags.sort {
        let wanted = sort.to_uppercase();
        if !bgnz_api::SORTS.contains(&wanted.as_str()) {
            return Err(AppError::usage(format!(
                "{sort:?} is not a sort; the index takes {}",
                bgnz_api::SORTS.join(", ")
            )));
        }
    }
    let query = query
        .with_limit(flags.limit)
        .with_offset(flags.offset)
        .with_sort(flags.sort.as_ref().map(|s| s.to_uppercase()))
        // The site hides out-of-stock products on both its grids, so matching
        // it is the default and `--all` is the deviation.
        .in_stock_only(!flags.all);

    let listing = app.client()?.listing(&query).await?;
    let mut out = app.out();
    emit(&mut out, &ProductList::new(&listing, flags.facets))?;
    Ok(())
}
