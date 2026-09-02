//! `wwnz orders` -- past orders, and what this account buys regularly.

use anyhow::Result;

use crate::api::SearchBy;
use crate::app::App;
use crate::cli::OrdersCommand;
use crate::commands::io::print_json;
use crate::output;

pub async fn run(app: &App, cmd: &OrdersCommand) -> Result<()> {
    match cmd {
        OrdersCommand::List { limit, filter } => {
            let client = app.account_client()?;
            let page = client.orders(*limit, *filter).await?;

            if app.json {
                print_json(&serde_json::json!({
                    "filter": filter.label(),
                    "count": page.orders.len(),
                    "total": page.total,
                    "orders": page.orders,
                }));
                return Ok(());
            }
            output::print_orders(&page);
            Ok(())
        }

        // "Buy it again" is a product search with an account-scoped selector,
        // so it renders as a product list rather than as order history.
        OrdersCommand::Previous { list } => {
            crate::commands::products::list(app, SearchBy::BuyAgain, false, list).await
        }
    }
}
