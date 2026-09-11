//! Showing the two fascias to a person.
//!
//! Every type here is a [`cli_kit::View`] over a [`bgnz_api`] type, which is
//! the whole shape of this layer: the protocol knows nothing about rendering,
//! the rendering knows nothing about HTTP, and `--json` falls out of the same
//! struct the text renderer reads rather than being written twice.
//!
//! These live in the app rather than in a library because there is only one
//! consumer. When a second general-merchandise retailer wants them, they move
//! to `packages/` -- and not before.

mod account;
mod cart;
mod categories;
mod orders;
mod product;
mod products;
mod stock;
mod stores;

pub use account::{AuthStatus, BannerAuth, LoyaltyView};
pub use cart::{CartView, WishlistView};
pub use categories::CategoryTree;
pub use orders::{OrderList, OrderView, ReceiptList};
pub use product::ProductView;
pub use products::ProductList;
pub use stock::{BasketStock, StockList, StockRow};
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

/// `$79.99`, or an em dash when there is no number.
pub(crate) fn money(value: Option<f64>) -> String {
    value.map_or_else(|| "—".into(), |v| format!("${v:.2}"))
}

pub(crate) fn money_of(m: Option<&bgnz_api::Money>) -> String {
    money(m.map(|m| m.value))
}

/// The price, with the crossed-out one alongside when something is reduced.
pub(crate) fn price_label(price: Option<f64>, was: Option<f64>) -> String {
    match (price, was) {
        // Only claim a saving when the maths is right: the index sends the
        // ticket price and the selling price as the same number for everything
        // that is not on special.
        (Some(now), Some(was)) if was > now => {
            format!("${now:.2} (was ${was:.2}, save ${:.2})", was - now)
        }
        (Some(now), _) => format!("${now:.2}"),
        (None, _) => "—".into(),
    }
}

/// A whole-number quantity without its `.0`.
///
/// Magento types quantity as a float and every real cart line is an integer,
/// so `2` rather than `2.0` -- while still showing a fraction if one ever turns
/// up rather than silently rounding it away.
pub(crate) fn quantity(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

/// `yes` / `no` / `—`, plain.
///
/// Plain on purpose: a coloured cell is measured by its bytes and breaks
/// `comfy-table`'s column rules.
pub(crate) fn yes_no(value: Option<bool>) -> String {
    match value {
        Some(true) => "yes".into(),
        Some(false) => "no".into(),
        None => "—".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quantity_loses_the_float_it_arrived_as() {
        assert_eq!(quantity(2.0), "2");
        assert_eq!(quantity(1.5), "1.5", "a fraction is shown, not rounded off");
    }

    #[test]
    fn a_saving_is_only_claimed_when_the_price_actually_fell() {
        assert_eq!(price_label(Some(79.99), Some(79.99)), "$79.99");
        assert_eq!(
            price_label(Some(59.99), Some(79.99)),
            "$59.99 (was $79.99, save $20.00)"
        );
        assert_eq!(price_label(None, Some(79.99)), "—");
    }
}
