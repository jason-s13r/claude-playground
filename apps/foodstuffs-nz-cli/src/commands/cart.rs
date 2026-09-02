//! `fsnz cart` -- reading and changing the signed-in shopping cart.

use anyhow::{bail, Context, Result};

use crate::app::App;
use crate::banner::Banner;
use crate::cli::CartCommand;
use crate::commands::io::print_json;
use crate::domain::cart::{Change, SaleType};
use crate::output;

/// Cart operations. Every mutation returns the resulting cart, so the effect of
/// a change is always shown rather than assumed.
pub async fn run(app: &App, banner: Banner, cmd: &CartCommand) -> Result<()> {
    let (client, _) = app.client(banner, false, true).await?;

    // A quantity of zero is how the API removes a line, so remove, update and
    // add are all the same call.
    let cart = match cmd {
        CartCommand::List => client.cart().await?,
        CartCommand::Clear { force } => {
            if !force {
                bail!("`cart clear` empties the whole cart; pass --force to confirm");
            }
            client.cart_clear().await?;
            client.cart().await?
        }
        CartCommand::Remove { sku } => {
            let change = Change {
                sale_type: SaleType::infer(sku),
                sku: sku.clone(),
                quantity: 0,
            };
            client.cart_apply(&[change]).await?
        }
        CartCommand::Add {
            sku,
            quantity,
            unit,
        } => {
            let sale_type = resolve_sale_type(sku, unit.as_deref())?;
            let quantity = match quantity {
                Some(q) => *q,
                // Guessing a weight would be guessing how much someone wants to
                // buy, so only counted items get a default.
                None if sale_type == SaleType::Units => 1,
                None => bail!(
                    "{sku} is sold by weight; give a quantity in grams, e.g. \
                     `fsnz cart add {sku} 300`"
                ),
            };
            if quantity == 0 {
                bail!("quantity 0 removes the line; use `fsnz cart remove {sku}`");
            }
            // The API sets rather than increments, so an add has to know what is
            // already there.
            let existing = client
                .cart()
                .await?
                .items
                .into_iter()
                .find(|i| i.sku.eq_ignore_ascii_case(sku))
                .map(|i| i.quantity)
                .unwrap_or(0);
            client
                .cart_apply(&[Change {
                    sku: sku.clone(),
                    quantity: existing + quantity,
                    sale_type,
                }])
                .await?
        }
        CartCommand::Update {
            sku,
            quantity,
            unit,
        } => {
            let sale_type = resolve_sale_type(sku, unit.as_deref())?;
            client
                .cart_apply(&[Change {
                    sku: sku.clone(),
                    quantity: *quantity,
                    sale_type,
                }])
                .await?
        }
    };

    if app.json {
        print_json(&serde_json::to_value(&cart).unwrap_or(serde_json::Value::Null));
        return Ok(());
    }

    output::print_cart(&cart, banner);

    // The cart is tied to its own store, which may not be the one searches are
    // priced against. Silently mixing the two would be confusing.
    if let (Some(cart_store), Some(configured)) = (
        cart.store_id.as_deref(),
        app.config.store_id(banner, app.store_flag.as_deref()),
    ) {
        if !cart_store.eq_ignore_ascii_case(&configured) {
            println!("\nNote: cart store is {cart_store}; searches price against {configured}.");
        }
    }
    Ok(())
}

fn resolve_sale_type(sku: &str, unit: Option<&str>) -> Result<SaleType> {
    match unit {
        Some(u) => SaleType::parse(u)
            .with_context(|| format!("unknown --unit '{u}' (expected 'units' or 'weight')")),
        None => Ok(SaleType::infer(sku)),
    }
}
