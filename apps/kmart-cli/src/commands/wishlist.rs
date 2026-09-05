//! `wishlist` -- what is saved for later.
//!
//! Reading and adding only. Removing is not here because the operation the
//! site uses for it was never observed and the gateway has introspection
//! disabled, so its name cannot be discovered -- see the `kmart-api` README.
//! An invented name would compile and fail at runtime, which is worse than not
//! offering it.

use cli_kit::emit;

use crate::app::App;
use crate::cli::WishlistAction;
use crate::error::AppResult;
use crate::views::WishlistView;

pub async fn run(app: &App, action: Option<WishlistAction>) -> AppResult<()> {
    let client = app.client().await?;
    let wishlist = match action.unwrap_or(WishlistAction::List) {
        WishlistAction::List => client.wishlist().await?,
        WishlistAction::Add { keycode, quantity } => {
            client.wishlist_add(&keycode, quantity).await?
        }
    };

    let mut out = app.out();
    Ok(emit(
        &mut out,
        &WishlistView {
            wishlist: &wishlist,
        },
    )?)
}
