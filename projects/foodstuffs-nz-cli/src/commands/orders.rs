//! `fsnz orders` -- past orders, what was in them, and what to buy again.

use anyhow::{bail, Result};

use crate::api::Client;
use crate::app::App;
use crate::banner::Banner;
use crate::cli::OrdersCommand;
use crate::commands::io::print_json;
use crate::domain::order::Source;
use crate::output::{self, plural};

/// Order history, like the cart, belongs to an account rather than a store, so
/// every one of these needs a logged-in token.
pub async fn run(app: &App, banner: Banner, cmd: &OrdersCommand) -> Result<()> {
    let (client, _) = app.client(banner, false, true).await?;

    match cmd {
        OrdersCommand::List { limit, source } => list(app, banner, &client, *limit, *source).await,
        OrdersCommand::Show { order, source } => show(app, banner, &client, order, *source).await,
        OrdersCommand::Previous {
            limit,
            include_cart,
        } => previous(app, banner, &client, *limit, !*include_cart).await,
    }
}

async fn list(
    app: &App,
    banner: Banner,
    client: &Client,
    limit: u32,
    source: Option<Source>,
) -> Result<()> {
    let page = client.orders(limit, source).await?;

    if app.json {
        print_json(&serde_json::json!({
            "banner": banner.id(),
            "source": source,
            "count": page.orders.len(),
            "total_available": page.total,
            "orders": page.orders,
        }));
        return Ok(());
    }

    if page.orders.is_empty() {
        let kind = source
            .map(|s| format!("{} ", s.label()))
            .unwrap_or_default();
        println!("No {kind}orders on this {} account.", banner.name());
        return Ok(());
    }

    output::print_orders(&page.orders, banner);
    if page.total as usize > page.orders.len() {
        println!(
            "showing {} of {} orders; raise --limit for more.",
            page.orders.len(),
            page.total
        );
    }
    Ok(())
}

async fn show(
    app: &App,
    banner: Banner,
    client: &Client,
    reference: &str,
    source: Option<Source>,
) -> Result<()> {
    let (id, source) = resolve(client, reference, source).await?;
    let order = client.order(&id, source).await?;

    if app.json {
        print_json(&serde_json::to_value(&order).unwrap_or(serde_json::Value::Null));
        return Ok(());
    }
    output::print_order(&order, banner);
    Ok(())
}

async fn previous(
    app: &App,
    banner: Banner,
    client: &Client,
    limit: u32,
    exclude_cart: bool,
) -> Result<()> {
    let lines = client.previous_purchases(limit, exclude_cart).await?;

    if app.json {
        print_json(&serde_json::json!({
            "banner": banner.id(),
            "count": lines.len(),
            "products": lines,
        }));
        return Ok(());
    }

    if lines.is_empty() {
        println!("Nothing bought before on this {} account.", banner.name());
        return Ok(());
    }
    output::print_previous(&lines, banner);
    Ok(())
}

/// Turn what someone typed into an order id and the endpoint to read it from.
///
/// Real ids are 150 characters of path, so nobody is going to type one: a small
/// number is read as a position in `fsnz orders list` instead, which costs one
/// extra request to look up. Positions shift as new orders arrive, which is
/// fine for the terminal and why `--json` prints the ids.
async fn resolve(
    client: &Client,
    reference: &str,
    source: Option<Source>,
) -> Result<(String, Source)> {
    if let Some(position) = as_position(reference) {
        if position == 0 {
            bail!("positions start at 1; `fsnz orders list` numbers them");
        }
        let page = client.orders(position, source).await?;
        let Some(order) = page.orders.get(position as usize - 1) else {
            bail!(
                "there is no order {position}: this account has {} order{}",
                page.total,
                plural(page.total as usize)
            );
        };
        return Ok((
            order.id.clone(),
            source.unwrap_or_else(|| order.resolved_source()),
        ));
    }
    Ok((
        reference.to_string(),
        source.unwrap_or_else(|| Source::infer(reference)),
    ))
}

/// A short number is a position; anything longer is an id, even an all-digit
/// one, because positions never run that high.
fn as_position(reference: &str) -> Option<u32> {
    if reference.len() > 3 {
        return None;
    }
    reference.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_numbers_are_positions_and_long_ones_are_ids() {
        assert_eq!(as_position("1"), Some(1));
        assert_eq!(as_position("16"), Some(16));
        assert_eq!(as_position("0"), Some(0), "caught later, with a message");
        assert_eq!(as_position("1234567890"), None, "an all-digit order id");
        assert_eq!(as_position("region/fsni/banner/NW"), None);
    }
}
