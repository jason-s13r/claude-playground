//! `product` and `stock` -- one product, and where it is.

use cli_kit::emit;

use crate::app::App;
use crate::error::{AppError, AppResult};
use crate::views::{ProductDetailView, StockList};

pub async fn show(app: &App, pid: &str, select: &[String]) -> AppResult<()> {
    let client = app.client()?;

    let detail = if select.is_empty() {
        client.product(pid).await?
    } else {
        // Each choice is applied to the page, and each answer is a different
        // variant with its own price and stock -- so they are applied in order
        // rather than all at once.
        let mut pdp = client.pdp(pid).await?;
        let mut detail = client.product(pid).await?;
        for choice in select {
            let (axis, value) = choice.split_once('=').ok_or_else(|| {
                AppError::usage(format!(
                    "{choice:?} is not a selection; write it as `--select size=M`"
                ))
            })?;
            pdp.detail = detail;
            detail = client.select(&pdp, axis.trim(), value.trim()).await?;
        }
        detail
    };

    emit(&mut app.out(), &ProductDetailView::new(&detail))?;
    Ok(())
}

pub async fn stock(app: &App, pid: &str, region: Option<&str>) -> AppResult<()> {
    let client = app.client()?;
    // Flag, then the region the chosen store is in, then everywhere. Resolved
    // before the request either way: the endpoint answers an unknown region
    // with an empty list, which reads as "no store has it".
    let code = match region.or(app.config.region.as_deref()) {
        Some(text) => Some(
            twlnz_api::region(text)
                .ok_or_else(|| twlnz_api::Error::NoSuchStore(text.to_string()))?,
        ),
        None => None,
    };

    let pdp = client.pdp(pid).await?;
    let stores = client.stock(&pdp, code).await?;
    emit(
        &mut app.out(),
        &StockList {
            pid,
            product: Some(&pdp.detail.product.name)
                .filter(|n| !n.is_empty())
                .map(String::as_str),
            region: code,
            stores: &stores,
        },
    )?;
    Ok(())
}
