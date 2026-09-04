//! `orders` -- the history, one order, and what to restock.

use cli_kit::{emit, Out, View};
use gsnz_core::{OrderFilter, OrderLine};
use gsnz_ui::{OrderDetail, OrderList};
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::cli::OrderAction;
use crate::error::{AppError, AppResult};

pub async fn run(app: &App, action: OrderAction) -> AppResult<()> {
    let handle = app.handle()?;
    let mut out = app.out();
    match action {
        OrderAction::List { limit, filter } => {
            let orders = handle.orders(filter, limit).await?;
            emit(
                &mut out,
                &OrderList::new(&orders).next("Show one: wwnz orders show <number>"),
            )?;
        }
        OrderAction::Show { order } => {
            let id = match position(&order) {
                // A position is resolved against the list rather than against a
                // cached one: a stale cache would show the wrong order, and the
                // extra call costs less than that mistake.
                Some(n) => {
                    let orders = handle.orders(OrderFilter::All, n).await?;
                    orders
                        .get(n as usize - 1)
                        .map(|o| o.id.clone())
                        .ok_or_else(|| {
                            AppError::usage(format!(
                                "there is no order {n}: the history has {}",
                                orders.len()
                            ))
                        })?
                }
                None => order,
            };
            let detail = handle.order(&id).await?;
            emit(&mut out, &OrderDetail(&detail))?;
        }
        OrderAction::Previous {
            limit,
            include_cart,
        } => {
            let lines = handle.previous_purchases(limit, !include_cart).await?;
            emit(&mut out, &Previous { lines: &lines })?;
        }
    }
    Ok(())
}

/// `orders show 3` means the third row of `orders list`.
///
/// Only a small bare number counts: an order number is a long digit string and
/// treating one as a position would fetch a different order entirely.
fn position(arg: &str) -> Option<u32> {
    let n: u32 = arg.parse().ok()?;
    (arg.len() <= 2 && n >= 1).then_some(n)
}

#[derive(Serialize)]
struct Previous<'a> {
    lines: &'a [OrderLine],
}

impl View for Previous<'_> {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        if self.lines.is_empty() {
            return writeln!(out, "Nothing bought before.");
        }
        for line in self.lines {
            let price = line
                .total_cents
                .map(gsnz_core::dollars)
                .unwrap_or_else(|| "-".into());
            let sku = out.dim(&line.sku);
            writeln!(out, "{price:>9}  {}  {sku}", line.name)?;
        }
        Ok(())
    }

    fn json(&self) -> cli_kit::serde_json::Value {
        // The bare array, so `wwnz orders previous --json | jq '.[0]'` works.
        cli_kit::serde_json::to_value(self.lines).unwrap_or(cli_kit::serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_digit_string_is_an_order_id_not_a_position() {
        assert_eq!(position("3"), Some(3));
        assert_eq!(position("12"), Some(12));
        assert_eq!(position("100"), None);
        assert_eq!(position("2409281234"), None);
        assert_eq!(position("0"), None);
        assert_eq!(position("WW-123"), None);
    }
}
