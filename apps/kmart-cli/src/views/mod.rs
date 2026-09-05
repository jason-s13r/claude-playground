//! Showing Kmart to a person.
//!
//! Every type here is a [`cli_kit::View`] over a [`kmart_api`] type, which is
//! the whole shape of this layer: the protocol knows nothing about rendering,
//! the rendering knows nothing about HTTP, and `--json` falls out of the same
//! struct the text renderer reads rather than being written twice.
//!
//! These live in the app rather than in a library because there is only one
//! consumer. When a second general-merchandise retailer wants them, they move
//! to `packages/` -- and not before.

mod cart;
mod categories;
mod orders;
mod product;
mod products;
mod stock;
mod stores;
mod wishlist;

pub use cart::CartView;
pub use categories::CategoryTree;
pub use orders::OrderList;
pub use product::ProductDetailView;
pub use products::{FacetList, ProductList};
pub use stock::StockView;
pub use stores::{StoreList, StoreView};
pub use wishlist::WishlistView;

use cli_kit::{plural, Out};
use kmart_api::{Price, Product};
use std::io::Write;

/// `3 stores. Select one: <what the caller said to run>`.
///
/// Shared by every listing so the shape is the same, and so the command half
/// is the caller's to supply -- these types do not know what the binary is
/// called.
pub(crate) fn write_count(
    out: &mut Out,
    count: usize,
    noun: &str,
    next: Option<&str>,
) -> std::io::Result<()> {
    let noun = format!("{noun}{}", plural(count));
    write_counted(out, count, &noun, next)
}

/// The same, for a noun `cli_kit::plural` cannot make: it appends an `s`, and
/// "categorys" is not a word.
pub(crate) fn write_irregular_count(
    out: &mut Out,
    count: usize,
    singular: &str,
    plural_form: &str,
    next: Option<&str>,
) -> std::io::Result<()> {
    let noun = if count == 1 { singular } else { plural_form };
    write_counted(out, count, noun, next)
}

fn write_counted(
    out: &mut Out,
    count: usize,
    noun: &str,
    next: Option<&str>,
) -> std::io::Result<()> {
    match next {
        Some(next) => writeln!(out, "{count} {noun}. {next}"),
        None => writeln!(out, "{count} {noun}."),
    }
}

/// `$39.00`, in whatever currency the country uses.
///
/// The symbol is the same for both and the currency is stated once per
/// listing rather than on every row, which is what the site does and what
/// keeps a price column narrow enough to compare down.
pub(crate) fn money(price: Price) -> String {
    format!("${:.2}", price.dollars())
}

/// The price, with the crossed-out one alongside when something is reduced.
pub(crate) fn price_label(product: &Product) -> String {
    let Some(now) = product.price else {
        return "—".to_string();
    };
    match product.was {
        Some(was) => format!("{} (was {})", money(now), money(was)),
        None => money(now),
    }
}

/// `Anko` or a dash. Enough products have no brand that an empty cell reads as
/// a bug.
pub(crate) fn or_dash(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => v.to_string(),
        None => "—".to_string(),
    }
}
