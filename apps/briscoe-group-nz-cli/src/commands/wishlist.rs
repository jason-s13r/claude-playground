//! `wishlist`.

use cli_kit::emit;

use crate::app::App;
use crate::cli::WishlistAction;
use crate::error::AppResult;
use crate::views::WishlistView;

pub async fn run(app: &App, action: WishlistAction) -> AppResult<()> {
    let client = app.client()?;
    match action {
        WishlistAction::List => {}
        WishlistAction::Add { sku, quantity } => {
            let count = client.wishlist_add(&sku, quantity).await?;
            println!(
                "Added {sku} to the {} wishlist ({count} items).",
                app.banner
            );
            return Ok(());
        }
        WishlistAction::Remove { item } => {
            // The list's own id, which the storefront needs and only the list
            // itself can supply.
            let list = client.wishlist().await?;
            let count = client.wishlist_remove(&list.id, &item).await?;
            println!("Removed it ({count} items left).");
            return Ok(());
        }
    }
    let wishlist = client.wishlist().await?;
    let mut out = app.out();
    emit(&mut out, &WishlistView::new(&wishlist, app.banner))?;
    Ok(())
}
