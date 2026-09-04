//! Showing The Warehouse to a person.
//!
//! Every type here is a [`cli_kit::View`] over a [`twlnz_api`] type, which is
//! the whole shape of this layer: the protocol knows nothing about rendering,
//! the rendering knows nothing about HTTP, and `--json` falls out of the same
//! struct the text renderer reads rather than being written twice.
//!
//! These live in the app rather than in a library because there is only one
//! consumer. When a second general-merchandise retailer wants them, they move
//! to `packages/` -- and not before.

mod cart;
mod departments;
mod product;
mod products;
mod stock;
mod stores;

pub use cart::CartView;
pub use departments::DepartmentTree;
pub use product::ProductDetailView;
pub use products::ProductList;
pub use stock::StockList;
pub use stores::{StoreList, StoreView};

use cli_kit::{plural, Out};
use std::io::Write;

/// `3 stores. Select one: <what the caller said to run>`.
///
/// Shared by every listing so the shape is the same, and so the command half is
/// the caller's to supply -- these types do not know what the binary is called.
pub(crate) fn write_count(
    out: &mut Out,
    count: usize,
    noun: &str,
    next: Option<&str>,
) -> std::io::Result<()> {
    match next {
        Some(next) => writeln!(out, "{count} {noun}{}. {next}", plural(count)),
        None => writeln!(out, "{count} {noun}{}.", plural(count)),
    }
}

/// The price, with the crossed-out one alongside when something is reduced.
pub(crate) fn price_label(product: &twlnz_api::Product) -> String {
    let Some(now) = product.price.label() else {
        return "—".to_string();
    };
    match product.was_price.as_ref().and_then(twlnz_api::Price::label) {
        Some(was) => match (
            product.price.value,
            product.was_price.as_ref().and_then(|p| p.value),
        ) {
            // Only claim a saving when both numbers parsed and the maths is
            // right; a scraped price can be a phrase rather than an amount.
            (Some(n), Some(w)) if w > n => format!("{now} (was {was}, save ${:.2})", w - n),
            _ => format!("{now} (was {was})"),
        },
        None => now,
    }
}

/// How stock reads, in a table cell.
///
/// **Plain, deliberately.** `comfy-table` measures a cell by its bytes, so a
/// string carrying ANSI colour codes is counted as wider than it draws and the
/// column rules stop lining up. Nothing in `gsnz-ui` colours a cell either, for
/// the same reason. Colour belongs outside the table -- see [`stock_colored`].
pub(crate) fn stock_label(availability: &twlnz_api::Availability) -> &'static str {
    availability.summary()
}

/// The same, coloured, for a line that is not inside a table.
pub(crate) fn stock_colored(out: &Out, availability: &twlnz_api::Availability) -> String {
    match availability.summary() {
        "in stock" => out.good("in stock"),
        // Not a warning and not a failure: it is orderable, just not by post.
        "in store" => out.warn("in store"),
        "sold out" => out.bad("sold out"),
        other => out.dim(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::Format;
    use twlnz_api::{Availability, Price, Product};

    #[test]
    fn a_reduced_price_says_what_was_saved() {
        let p = Product {
            price: Price::from_display("$5.00"),
            was_price: Some(Price::from_display("$9.00")),
            ..Product::default()
        };
        assert_eq!(price_label(&p), "$5.00 (was $9.00, save $4.00)");
    }

    #[test]
    fn a_price_that_is_a_phrase_still_prints_without_claiming_a_saving() {
        let p = Product {
            price: Price::from_display("See in store"),
            was_price: Some(Price::from_display("See in store")),
            ..Product::default()
        };
        assert_eq!(price_label(&p), "See in store (was See in store)");
    }

    #[test]
    fn a_product_with_no_price_prints_a_dash_rather_than_nothing() {
        assert_eq!(price_label(&Product::default()), "—");
    }

    #[test]
    fn in_store_stock_does_not_read_as_sold_out() {
        let in_store = Availability {
            online: Some(false),
            in_store: Some(true),
            ..Availability::default()
        };
        assert_eq!(stock_label(&in_store), "in store");
        assert_eq!(stock_label(&Availability::default()), "-");
    }

    #[test]
    fn a_table_cell_carries_no_escape_codes() {
        // comfy-table measures a cell by its bytes, so a coloured one is
        // counted wider than it draws and the column rules stop lining up.
        let out = Out::buffer(Format::Text).with_color(true);
        let sold_out = Availability {
            online: Some(false),
            in_store: Some(false),
            ..Availability::default()
        };
        assert_eq!(stock_label(&sold_out), "sold out");
        assert!(
            stock_colored(&out, &sold_out).contains('\u{1b}'),
            "the non-table form still colours"
        );
    }
}
