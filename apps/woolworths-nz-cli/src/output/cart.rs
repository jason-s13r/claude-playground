//! Rendering the cart.

use owo_colors::{OwoColorize, Stream};

use crate::domain::cart::{format_quantity, Cart};
use crate::domain::dollars;
use crate::output::{plural, store_heading, table};

pub fn print_cart(cart: &Cart) {
    if cart.is_empty() {
        println!("The cart is empty.");
        print_problems(cart);
        return;
    }

    let mut t = table();
    t.set_header(vec!["SKU", "Product", "Qty", "Unit", "Total"]);
    for line in &cart.lines {
        t.add_row(vec![
            line.sku.clone(),
            line.title(),
            format_quantity(line.quantity),
            line.unit_price_cents
                .map(dollars)
                .unwrap_or_else(|| "—".into()),
            line.total_cents.map(dollars).unwrap_or_else(|| "—".into()),
        ]);
    }
    println!("{t}");

    let where_from = match (&cart.store_name, &cart.fulfilment_method) {
        (Some(store), Some(method)) => format!("{} ({method})", store_heading(Some(store))),
        (Some(store), None) => store_heading(Some(store)),
        _ => store_heading(None),
    };
    println!("{where_from}");

    // `total_items` counts quantities and `lines.len()` counts products; both
    // are worth saying, since "3 items" and "3 products" are rarely the same.
    println!(
        "{} item{} across {} product{}",
        format_quantity(cart.total_items),
        // Not `plural`, which counts in whole numbers: 0.3 of a kilogram is
        // "0.3 items", not "0.3 item".
        if cart.total_items == 1.0 { "" } else { "s" },
        cart.lines.len(),
        plural(cart.lines.len()),
    );

    // The rows above add up to the lines, not to the order subtotal -- the
    // site folds delivery and pickup fees into that one. Showing the fee on its
    // own line is what makes the column reconcile with the total.
    if let Some(items) = cart.items_cents {
        println!("Items:    {}", dollars(items));
    }
    if let Some(fees) = cart.fees_cents() {
        println!("Fees:     {}", dollars(fees));
    }
    if let Some(discount) = cart.discount_cents {
        println!("Discount: -{}", dollars(discount));
    }
    // What is actually owed can differ from the subtotal again once loyalty
    // spend is on, so it is printed rather than inferred.
    if let Some(pay) = cart.to_pay_cents {
        println!("To pay:   {}", dollars(pay));
    }

    print_problems(cart);
}

fn print_problems(cart: &Cart) {
    for problem in &cart.problems {
        println!(
            "{} {problem}",
            "warning:".if_supports_color(Stream::Stdout, |t| t.yellow())
        );
    }
}
