//! `product` and `stock`.
//!
//! `stock` is the one that carries a surprise. A configurable product -- most
//! of Rebel Sport -- has **no barcode of its own**, and the availability
//! service identifies a product by barcode, so there is nothing to ask about
//! until a variant is chosen. Rather than refusing, this asks about every
//! variant that has one, which is what a person standing in a shop actually
//! wants: not "is this shoe collectable" but "which sizes are".
//!
//! That costs one request per variant, because the service answers **per
//! basket**: sending all the variants at once would come back with one verdict
//! for the lot, decided by the worst of them.

use cli_kit::emit;

use crate::app::App;
use crate::error::{AppError, AppResult};
use crate::views::{ProductView, StockList, StockRow};

/// Accept a pasted product URL as well as a SKU.
///
/// Both sites spell a product page `/product/<sku>/<slug>/`, so the SKU can be
/// lifted without a round trip to `ResolveURL`.
fn sku_of(input: &str) -> &str {
    if !input.contains("://") {
        return input;
    }
    input
        .split("/product/")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(input)
}

pub async fn show(app: &App, sku: &str) -> AppResult<()> {
    let product = app.client()?.product(sku_of(sku)).await?;
    let mut out = app.out();
    emit(&mut out, &ProductView::new(&product, app.banner))?;
    Ok(())
}

pub async fn stock(app: &App, sku: &str, store: Option<i64>) -> AppResult<()> {
    let store_id = app.store(store).ok_or_else(|| {
        AppError::usage(
            "no store chosen; pass --store <id>, or set one with `bgnz store set <id>` \
             (`bgnz stores` lists them)",
        )
    })?;

    let client = app.client()?;
    let product = client.product(sku_of(sku)).await?;
    let stores = client.stores().await?;
    let store = stores
        .iter()
        .find(|s| s.id == store_id)
        .ok_or_else(|| bgnz_api::Error::NoSuchStore(store_id.to_string()))?;

    let mut rows = Vec::new();
    for (variant_sku, fulfilment) in product.stockable() {
        let answer = client.stock(variant_sku, fulfilment, store_id).await?;
        rows.push(StockRow {
            sku: variant_sku.to_string(),
            variant: product.variant(variant_sku).map(|v| v.label()),
            pickup_status: answer.pickup_status,
            message: answer.message,
        });
    }

    let mut out = app.out();
    emit(
        &mut out,
        &StockList {
            sku: product.sku.clone(),
            store: store.name.clone(),
            store_id,
            rows,
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_product_url_is_read_as_a_sku() {
        assert_eq!(
            sku_of("https://www.briscoes.co.nz/product/1132853/tudo-home-vibe-ottoman-ivory/"),
            "1132853"
        );
        assert_eq!(
            sku_of("https://www.rebelsport.co.nz/product/8237741/asics-jetray-pro/"),
            "8237741"
        );
    }

    #[test]
    fn a_bare_sku_is_left_alone() {
        assert_eq!(sku_of("1132853"), "1132853");
        // Not a product URL: better to send it and let the storefront say no
        // than to guess at a SKU that was never there.
        assert_eq!(
            sku_of("https://www.briscoes.co.nz/kitchen/"),
            "https://www.briscoes.co.nz/kitchen/"
        );
    }
}
