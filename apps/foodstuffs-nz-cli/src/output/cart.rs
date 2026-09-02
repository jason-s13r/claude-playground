//! Rendering the cart.

use comfy_table::Cell;

use crate::banner::Banner;
use crate::domain::cart::Cart;
use crate::domain::dollars;
use crate::output::{store_heading, table};

pub fn print_cart(cart: &Cart, banner: Banner) {
    let where_ = store_heading(cart.store_name.as_deref(), banner);

    if cart.is_empty() {
        println!("{where_}\n\nThe cart is empty.");
        return;
    }

    println!("{where_}\n");
    let mut t = table();
    t.set_header(vec!["Qty", "Product", "SKU", "Line total"]);
    for item in &cart.items {
        t.add_row(vec![
            Cell::new(item.quantity_label()),
            Cell::new(&item.name),
            Cell::new(&item.sku),
            Cell::new(dollars(item.line_total_cents)),
        ]);
    }
    println!("{t}");

    let mut money = vec![("Subtotal", cart.subtotal_cents)];
    if cart.service_fee_cents != 0 {
        money.push(("Service fee", cart.service_fee_cents));
    }
    if cart.bag_fee_cents != 0 {
        money.push(("Bag fee", cart.bag_fee_cents));
    }
    if cart.promo_discount_cents != 0 {
        money.push(("Promo discount", -cart.promo_discount_cents));
    }
    if cart.subscription_discount_cents != 0 {
        money.push(("Subscription discount", -cart.subscription_discount_cents));
    }
    for (label, cents) in money {
        println!("  {label:<22} {:>9}", dollars(cents));
    }
    println!(
        "  {:<22} {:>9}",
        "Estimated total",
        dollars(cart.estimated_total_cents())
    );
    println!("\n{}", cart.summary());

    if !cart.unavailable.is_empty() {
        println!("\nUnavailable at this store:");
        for item in &cart.unavailable {
            println!("  {} {} ({})", item.quantity_label(), item.name, item.sku);
        }
    }
}
