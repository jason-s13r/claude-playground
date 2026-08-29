//! Rendering order history.

use comfy_table::Cell;

use crate::banner::Banner;
use crate::domain::dollars;
use crate::domain::order::{Order, OrderLine, OrderSummary};
use crate::output::{plural, store_heading, table};

/// Money that may not have come back at all: an order the API described without
/// a total is a missing column, not a zero.
fn total_label(cents: Option<i64>) -> String {
    cents.map(dollars).unwrap_or_else(|| "—".to_string())
}

/// Numbered, because the ids are 150 characters of path and `fsnz orders show`
/// takes the number.
pub fn print_orders(orders: &[OrderSummary], banner: Banner) {
    println!(
        "{} — {} order{}\n",
        banner.name(),
        orders.len(),
        plural(orders.len()),
    );
    let mut t = table();
    t.set_header(vec!["#", "Placed", "Store", "Where", "Total"]);
    for (i, o) in orders.iter().enumerate() {
        t.add_row(vec![
            Cell::new(i + 1),
            Cell::new(o.placed_label()),
            Cell::new(o.store_name.as_deref().unwrap_or("—")),
            Cell::new(o.source_label()),
            Cell::new(total_label(o.total_cents)),
        ]);
    }
    println!("{t}");
    println!("Show one: fsnz orders show <#>");
}

pub fn print_order(order: &Order, banner: Banner) {
    let s = &order.summary;
    println!("{}\n", store_heading(s.store_name.as_deref(), banner));

    let mut meta = vec![format!("Placed {}", s.placed_label())];
    meta.push(s.source_label().to_string());
    if let Some(status) = order.status.as_deref().filter(|v| !v.trim().is_empty()) {
        meta.push(status.to_string());
    }
    if let Some(method) = order.fulfilment.as_deref().filter(|v| !v.trim().is_empty()) {
        meta.push(method.to_string());
    }
    println!("{}", meta.join(" · "));
    if let Some(slot) = &order.timeslot {
        println!("Timeslot: {slot}");
    }
    if let Some(address) = &order.address {
        println!("To: {address}");
    }
    if !s.id.is_empty() {
        println!("Id: {}", s.id);
    }

    if order.lines.is_empty() {
        println!("\nNothing itemised on this order.");
        return;
    }

    let mut t = table();
    t.set_header(vec!["Qty", "Product", "SKU", "Line total"]);
    for line in &order.lines {
        t.add_row(vec![
            Cell::new(line.quantity_label()),
            Cell::new(line.title()),
            Cell::new(&line.sku),
            Cell::new(dollars(line.line_total_cents)),
        ]);
    }
    println!("\n{t}");

    // The lines usually add up to the total exactly, in which case saying so
    // twice is just noise.
    let items = order.lines_total_cents();
    let mut money = Vec::new();
    if Some(items) != s.total_cents {
        money.push(("Items", dollars(items)));
    }
    if order.service_fee_cents != 0 {
        money.push(("Service fee", dollars(order.service_fee_cents)));
    }
    if order.bag_fee_cents != 0 {
        money.push(("Bag fee", dollars(order.bag_fee_cents)));
    }
    money.push(("Total", total_label(s.total_cents)));
    for (label, amount) in money {
        println!("  {label:<22} {amount:>9}");
    }
    println!("\n{}", order.summary_line());
}

pub fn print_previous(lines: &[OrderLine], banner: Banner) {
    println!(
        "{} — {} product{} bought before\n",
        banner.name(),
        lines.len(),
        plural(lines.len()),
    );
    let mut t = table();
    t.set_header(vec!["Qty", "Product", "SKU", "Last paid"]);
    for line in lines {
        t.add_row(vec![
            Cell::new(line.quantity_label()),
            Cell::new(line.title()),
            Cell::new(&line.sku),
            Cell::new(dollars(line.line_total_cents)),
        ]);
    }
    println!("{t}");
    println!("What it cost last time, not today. Buy one again: fsnz cart add <sku>");
}
