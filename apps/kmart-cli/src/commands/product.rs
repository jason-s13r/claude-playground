//! `product` and `stock` -- one product, and where it can be had.

use cli_kit::emit;

use crate::app::App;
use crate::error::AppResult;
use crate::views::{ProductDetailView, StockView};

pub async fn show(app: &App, keycode: &str, no_stock: bool) -> AppResult<()> {
    let client = app.client().await?;
    let product = client.product(keycode).await?;

    // Stock is a second request against the gateway, which needs imported
    // cookies. A person who has not imported any should still be able to read
    // a price, so a challenge here is reported as a missing section rather
    // than as a failed command.
    let availability = match no_stock || product.marketplace() {
        true => None,
        false => match app.config.postcode.as_deref() {
            None => None,
            Some(postcode) => match client
                .availability(keycode, postcode, product.national_inventory)
                .await
            {
                Ok(a) => Some(a),
                Err(e) if e.is_challenge() || e.is_lapsed() => None,
                Err(e) => return Err(e.into()),
            },
        },
    };

    let mut out = app.out();
    Ok(emit(
        &mut out,
        &ProductDetailView {
            product: &product,
            country: app.country.name(),
            availability: availability.as_ref(),
            url: product.page(app.country),
        },
    )?)
}

pub async fn stock(app: &App, keycode: &str, postcode: Option<&str>) -> AppResult<()> {
    let postcode = app.postcode(postcode)?;
    let client = app.client().await?;

    // The product first, for its name and for `nationalInventory` -- which the
    // availability query needs and which is only ever found on the catalogue
    // record.
    let product = client.product(keycode).await?;
    let mut availability = client
        .availability(keycode, &postcode, product.national_inventory)
        .await?;

    // The gateway answers with location ids and no names, so each one is a
    // request of its own. Worth it here -- a table of bare ids is not an
    // answer to "where can I get this".
    for store in &mut availability.stores {
        if let Ok(detail) = client.store(&store.store_id).await {
            store.name = Some(detail.name);
        }
    }

    let mut out = app.out();
    Ok(emit(
        &mut out,
        &StockView {
            availability: &availability,
            product: Some(&product.name),
        },
    )?)
}
