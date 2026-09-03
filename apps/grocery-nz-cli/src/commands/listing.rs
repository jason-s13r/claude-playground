//! `search`, `specials` and `browse` -- one function, because they differ only
//! in what selects the products.

use cli_kit::emit;
use gsnz_core::{Search, SearchBy};
use gsnz_ui::ProductList;

use crate::app::App;
use crate::cli::Listing;
use crate::error::AppResult;

pub async fn run(app: &App, by: SearchBy, flags: Listing) -> AppResult<()> {
    let retailer = app.retailer()?;
    let handle = app.handle()?;
    let search = Search {
        by,
        specials_only: flags.specials,
        limit: flags.limit,
        size: flags.size,
        sort: flags.sort,
    };
    let found = handle.search(&search).await?;
    let store = app.config.retailer(retailer).store_id.clone();
    let view = ProductList::new(&found.products, retailer)
        .at(store.as_deref())
        .of(found.total);
    emit(&mut app.out(), &view)?;
    Ok(())
}
