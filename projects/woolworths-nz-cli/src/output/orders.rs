//! Rendering order history.

use crate::domain::dollars;
use crate::domain::order::OrderPage;
use crate::output::{plural, table};

pub fn print_orders(page: &OrderPage) {
    if page.orders.is_empty() {
        println!("No orders.");
        return;
    }

    let mut t = table();
    t.set_header(vec!["Order", "Placed", "Status", "Fulfilment", "Total"]);
    for o in &page.orders {
        // A pickup names a store and a delivery an address; either way the
        // method plus the destination is what identifies the order.
        let fulfilment = [o.method.as_deref(), o.destination.as_deref()]
            .into_iter()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" — ");
        t.add_row(vec![
            o.number.clone(),
            o.placed_on().unwrap_or_default(),
            o.status.clone().unwrap_or_default(),
            fulfilment,
            o.total_cents.map(dollars).unwrap_or_else(|| "—".into()),
        ]);
    }
    println!("{t}");

    let shown = page.orders.len();
    println!("{shown} order{}", plural(shown));
    if page.total as usize > shown {
        println!("showing {shown} of {}; raise --limit for more.", page.total);
    }
}
