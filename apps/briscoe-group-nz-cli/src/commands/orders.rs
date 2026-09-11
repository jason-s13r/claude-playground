//! `orders`.
//!
//! Two histories under one command, because a person does not think of them as
//! two: `list` is what was bought online and `receipts` is what was bought in a
//! shop. They come from different systems -- the storefront and SAP -- and
//! share no identifiers, which is why one cannot page into the other.

use cli_kit::emit;

use crate::app::App;
use crate::cli::OrderAction;
use crate::error::{AppError, AppResult};
use crate::views::{OrderList, OrderView, ReceiptList};

pub async fn run(app: &App, action: OrderAction) -> AppResult<()> {
    let client = app.client()?;
    let mut out = app.out();
    match action {
        OrderAction::List { page, limit } => {
            let orders = client.orders(page, limit).await?;
            emit(&mut out, &OrderList::new(&orders, app.banner))?;
        }
        OrderAction::Show { number } => {
            let order = client.order(&number).await?;
            emit(&mut out, &OrderView { order: &order })?;
        }
        OrderAction::Receipts { page, from, to } => {
            let receipts = client
                .receipts(page, from.as_deref(), to.as_deref())
                .await?;
            emit(&mut out, &ReceiptList::new(&receipts, app.banner))?;
        }
        OrderAction::Invoice { number } => {
            // The invoice id, not the order number: the storefront mints a
            // download token against the invoice, and an order can have more
            // than one.
            let order = client.order(&number).await?;
            let invoice = order.invoices.first().ok_or_else(|| {
                AppError::usage(format!("order {number} has no invoice to download"))
            })?;
            let url = client.invoice_pdf(&invoice.id).await?;
            println!("{url}");
        }
    }
    Ok(())
}
