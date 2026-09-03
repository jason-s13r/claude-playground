//! Past shops: a list, and one in detail.

use std::io::{self, Write};

use cli_kit::comfy_table::Cell;
use cli_kit::{plural, table, Out, View};
use gsnz_core::{dollars, Order, OrderSummary};
use serde::Serialize;

#[derive(Serialize)]
#[serde(transparent)]
pub struct OrderList<'a>(pub &'a [OrderSummary]);

impl View for OrderList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.0.is_empty() {
            return writeln!(out, "No orders on file.");
        }
        let mut t = table(&["#", "Placed", "Status", "Store", "Total"]);
        // Numbered so a following command can name one by position -- an order
        // id is long and, at one retailer, contains a slash.
        for (i, o) in self.0.iter().enumerate() {
            t.add_row(vec![
                Cell::new(i + 1),
                Cell::new(short_datetime(o.placed_at.as_deref())),
                Cell::new(o.status.clone().unwrap_or_default()),
                Cell::new(
                    o.store
                        .as_ref()
                        .and_then(|s| s.name.clone())
                        .unwrap_or_default(),
                ),
                Cell::new(o.total_cents.map(dollars).unwrap_or_else(|| "—".into())),
            ]);
        }
        writeln!(out, "{t}")?;
        writeln!(
            out,
            "{} order{}. Show one: gsnz orders show <number>",
            self.0.len(),
            plural(self.0.len())
        )
    }
}

#[derive(Serialize)]
#[serde(transparent)]
pub struct OrderDetail<'a>(pub &'a Order);

impl View for OrderDetail<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let order = self.0;
        writeln!(
            out,
            "{}",
            out.heading(&format!("Order {}", order.summary.id))
        )?;

        let mut facts: Vec<(&str, String)> = Vec::new();
        if let Some(placed) = order.summary.placed_at.as_deref() {
            facts.push(("Placed", short_datetime(Some(placed))));
        }
        if let Some(status) = order.summary.status.as_deref() {
            facts.push(("Status", status.to_string()));
        }
        if let Some(store) = order.summary.store.as_ref().and_then(|s| s.name.as_deref()) {
            facts.push(("Store", store.to_string()));
        }
        if let Some(f) = order.summary.fulfilment.as_deref() {
            facts.push(("Fulfilment", f.to_string()));
        }
        if let Some(slot) = order.timeslot.as_deref() {
            facts.push(("Timeslot", slot.to_string()));
        }
        if let Some(address) = order.address.as_deref() {
            facts.push(("Address", address.to_string()));
        }
        for (label, value) in facts {
            writeln!(out, "  {label:<12}{value}")?;
        }

        if !order.lines.is_empty() {
            writeln!(out)?;
            let mut t = table(&["Qty", "Product", "SKU", "Line total"]);
            for line in &order.lines {
                t.add_row(vec![
                    Cell::new(line.quantity.format()),
                    Cell::new(&line.name),
                    Cell::new(&line.sku),
                    Cell::new(line.total_cents.map(dollars).unwrap_or_else(|| "—".into())),
                ]);
            }
            writeln!(out, "{t}")?;
        }

        for adjustment in &order.adjustments {
            writeln!(
                out,
                "  {:<24}{:>10}",
                adjustment.label,
                dollars(adjustment.cents)
            )?;
        }
        if let Some(total) = order.summary.total_cents {
            writeln!(out, "  {:<24}{:>10}", "Total", dollars(total))?;
        }
        Ok(())
    }
}

/// `2026-09-03 14:05` out of whatever the retailer sent.
///
/// Truncated rather than parsed and reformatted: both retailers send store-local
/// time with no offset, so there is nothing to convert it to and a timezone
/// crate would only add a way to be wrong.
fn short_datetime(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return String::new();
    };
    let cleaned = raw.replace('T', " ");
    match cleaned.char_indices().nth(16) {
        Some((cut, _)) => cleaned[..cut].to_string(),
        None => cleaned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use gsnz_core::{Adjustment, OrderLine, Quantity, RetailerId, StoreRef};

    fn summary(id: &str, cents: Option<i64>) -> OrderSummary {
        OrderSummary {
            retailer: RetailerId::NewWorld,
            id: id.into(),
            placed_at: Some("2026-09-03T14:05:22.000Z".into()),
            total_cents: cents,
            status: Some("Delivered".into()),
            fulfilment: Some("Delivery".into()),
            store: Some(StoreRef {
                id: "4147".into(),
                name: Some("New World Thorndon".into()),
            }),
        }
    }

    fn render<V: View>(view: &V) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, view).unwrap();
        out.into_string()
    }

    #[test]
    fn timestamps_are_truncated_not_reformatted() {
        assert_eq!(
            short_datetime(Some("2026-09-03T14:05:22.000Z")),
            "2026-09-03 14:05"
        );
        assert_eq!(short_datetime(Some("2026-09-03")), "2026-09-03");
        assert_eq!(short_datetime(None), "");
    }

    #[test]
    fn the_list_numbers_orders_so_one_can_be_named_by_position() {
        let orders = vec![summary("ABC/123", Some(4500)), summary("DEF/456", None)];
        let text = render(&OrderList(&orders));
        assert!(text.contains("2 orders."), "{text}");
        assert!(text.contains("gsnz orders show <number>"), "{text}");
        assert!(text.contains("$45.00"), "{text}");
        assert!(text.contains("—"), "a missing total is not a zero: {text}");
    }

    #[test]
    fn no_orders_says_so() {
        assert_eq!(render(&OrderList(&[])), "No orders on file.\n");
    }

    #[test]
    fn detail_prints_the_facts_it_has_and_omits_the_rest() {
        let order = Order {
            summary: summary("ABC/123", Some(4500)),
            lines: vec![OrderLine {
                sku: "MILK".into(),
                key: "MILK".into(),
                name: "Blue Milk 2L".into(),
                brand: None,
                quantity: Quantity::units(2),
                total_cents: Some(900),
            }],
            address: None,
            timeslot: Some("Thu 4-6pm".into()),
            adjustments: vec![Adjustment {
                label: "Delivery".into(),
                cents: 800,
            }],
        };
        let text = render(&OrderDetail(&order));
        assert!(text.contains("Order ABC/123"), "{text}");
        assert!(text.contains("Timeslot"), "{text}");
        assert!(
            !text.contains("Address"),
            "absent facts are omitted: {text}"
        );
        assert!(text.contains("Blue Milk 2L"), "{text}");
        assert!(text.contains("Delivery"), "{text}");
        assert!(text.contains("$45.00"), "{text}");
    }
}
