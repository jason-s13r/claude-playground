//! `cart`.
//!
//! Every one of these needs an account, and the cart they act on is the
//! storefront's own -- the same basket the website shows, not a local one.

use cli_kit::emit;

use crate::app::App;
use crate::cli::CartAction;
use crate::commands::stores;
use crate::error::AppResult;
use crate::views::{BasketStock, CartView};

pub async fn run(app: &App, action: CartAction) -> AppResult<()> {
    let client = app.client()?;
    let cart_id = client.cart_id().await?;

    let cart = match action {
        CartAction::List => client.cart(&cart_id).await?,
        CartAction::Add { sku, quantity } => client.cart_add(&cart_id, &sku, quantity).await?,
        CartAction::Set { item, quantity } => client.cart_update(&cart_id, &item, quantity).await?,
        CartAction::Remove { item } => client.cart_remove(&cart_id, &item).await?,
        CartAction::Coupon { code } => client.cart_coupon(&cart_id, &code).await?,
        CartAction::Collect { store } => return collect(app, &cart_id, store).await,
    };

    let mut out = app.out();
    emit(&mut out, &CartView::new(&cart, app.banner))?;
    Ok(())
}

/// Whether the whole basket can be collected from one shop.
///
/// One request and one verdict, because that is what the service answers --
/// send three lines and the worst of them decides the result. Asking per line
/// would be a different question and three times the requests, which is what
/// `bgnz stock` is for.
async fn collect(app: &App, cart_id: &str, store: Option<i64>) -> AppResult<()> {
    let store_id = stores::require(app, store)?;
    let client = app.client()?;
    let cart = client.cart(cart_id).await?;

    let items: Vec<_> = cart
        .lines
        .iter()
        .map(|line| {
            (
                line.sku.clone(),
                line.fulfilment.clone(),
                line.quantity.max(1.0) as u32,
            )
        })
        .collect();

    let stock = client.basket_stock(&items, store_id).await?;
    let name = client
        .stores()
        .await?
        .into_iter()
        .find(|s| s.id == store_id)
        .map(|s| s.name)
        .unwrap_or_else(|| store_id.to_string());

    let mut out = app.out();
    emit(
        &mut out,
        &BasketStock {
            stock: &stock,
            store: name,
        },
    )?;
    Ok(())
}
