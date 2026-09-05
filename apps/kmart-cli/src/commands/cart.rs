//! `cart` -- the shopping bag.

use cli_kit::emit;

use crate::app::App;
use crate::cli::CartAction;
use crate::error::AppResult;
use crate::views::CartView;

pub async fn run(app: &App, action: Option<CartAction>) -> AppResult<()> {
    let client = app.client().await?;
    let cart = match action.unwrap_or(CartAction::List) {
        CartAction::List => client.cart().await?,
        CartAction::Add { keycode, quantity } => Some(
            client
                .cart_add(&keycode, quantity, app.config.postcode.as_deref())
                .await?,
        ),
        CartAction::Set { keycode, quantity } => Some(client.cart_set(&keycode, quantity).await?),
        CartAction::Remove { keycode } => Some(client.cart_remove(&keycode).await?),
    };

    let mut out = app.out();
    Ok(emit(
        &mut out,
        &CartView {
            cart: cart.as_ref(),
        },
    )?)
}
