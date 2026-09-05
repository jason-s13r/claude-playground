//! Past orders.

use std::io::{self, Write};

use cli_kit::{serde_json, table, Out, View};
use kmart_api::OrderPage;
use serde::Serialize;

#[derive(Serialize)]
pub struct OrderList<'a> {
    pub page: &'a OrderPage,
}

impl View for OrderList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.page.orders.is_empty() {
            return writeln!(out, "No orders.");
        }
        let mut t = table(&["Order", "Placed", "Status", "Total", "Tracking"]);
        for o in &self.page.orders {
            t.add_row(vec![
                o.id.clone(),
                // The date arrives as an ISO timestamp; the day is the part
                // anyone reads.
                super::or_dash(o.placed.as_deref().and_then(|d| d.split('T').next())),
                super::or_dash(o.status.as_deref()),
                o.total.map(super::money).unwrap_or_else(|| "—".into()),
                match o.tracking.is_empty() {
                    true => "—".to_string(),
                    false => o
                        .tracking
                        .iter()
                        .filter_map(|t| t.number.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                },
            ]);
        }
        writeln!(out, "{t}")?;
        let next = self
            .page
            .total
            .filter(|total| *total as usize > self.page.orders.len())
            .map(|total| format!("{total} in all; raise --limit for more."));
        super::write_count(out, self.page.orders.len(), "order", next.as_deref())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use kmart_api::{Order, Price, Tracking};

    fn render(page: &OrderPage) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &OrderList { page }).unwrap();
        out.into_string()
    }

    #[test]
    fn an_order_shows_the_day_rather_than_the_timestamp() {
        let page = OrderPage {
            orders: vec![Order {
                id: "NZ123456".into(),
                total: Some(Price::cents(7800)),
                status: Some("Shipped".into()),
                placed: Some("2026-09-01T04:12:33.000Z".into()),
                tracking: vec![Tracking {
                    number: Some("ABC123".into()),
                    carrier: Some("NZ Post".into()),
                    link: None,
                }],
            }],
            total: Some(1),
            next: None,
        };
        let text = render(&page);
        assert!(text.contains("NZ123456"), "{text}");
        assert!(text.contains("2026-09-01"), "{text}");
        assert!(!text.contains("T04:12"), "{text}");
        assert!(text.contains("ABC123"), "{text}");
        assert!(text.contains("$78.00"), "{text}");
    }

    #[test]
    fn no_orders_says_so() {
        let page = OrderPage {
            orders: Vec::new(),
            total: Some(0),
            next: None,
        };
        assert_eq!(render(&page), "No orders.\n");
    }
}
