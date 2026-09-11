//! The basket, and the saved-items list.

use std::io::{self, Write};

use bgnz_api::{Cart, Wishlist};
use cli_kit::{table, Out, View};
use serde::Serialize;

#[derive(Serialize)]
pub struct CartView<'a> {
    pub cart: &'a Cart,
    pub banner: String,
}

impl<'a> CartView<'a> {
    pub fn new(cart: &'a Cart, banner: bgnz_api::Banner) -> CartView<'a> {
        CartView {
            cart,
            banner: banner.name().to_string(),
        }
    }
}

impl View for CartView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let c = self.cart;
        if c.lines.is_empty() {
            return writeln!(out, "The {} cart is empty.", self.banner);
        }
        // The line uid, not the SKU: it is what `cart set` and `cart remove`
        // take, and a configurable product can be in the cart twice under one
        // SKU in two sizes.
        let mut t = table(&["Item", "SKU", "Product", "Qty", "Price", "Total"]);
        for line in &c.lines {
            t.add_row(vec![
                line.uid.clone(),
                line.sku.clone(),
                line.name.clone(),
                super::quantity(line.quantity),
                super::money_of(line.price.as_ref()),
                super::money_of(line.row_total.as_ref()),
            ]);
        }
        writeln!(out, "{t}")?;

        // Subtotal only when a discount moved it: with nothing applied it
        // repeats the total, and two identical numbers read as a mistake.
        let discounted = !c.discounts.is_empty()
            || match (&c.subtotal, &c.total) {
                (Some(sub), Some(total)) => (sub.value - total.value).abs() >= 0.005,
                _ => false,
            };
        if discounted {
            writeln!(out, "Subtotal {}", super::money_of(c.subtotal.as_ref()))?;
            for d in &c.discounts {
                writeln!(
                    out,
                    "{} -{}",
                    d.label.as_deref().unwrap_or("Discount"),
                    super::money(Some(d.amount.value))
                )?;
            }
        }
        if !c.coupons.is_empty() {
            writeln!(out, "Coupons {}", c.coupons.join(", "))?;
        }
        if let Some(total) = &c.total {
            writeln!(
                out,
                "{}",
                out.heading(&format!("Total {}", super::money(Some(total.value))))
            )?;
        }

        let broken: Vec<_> = c.lines.iter().filter(|l| !l.errors.is_empty()).collect();
        for line in broken {
            writeln!(
                out,
                "{} {}: {}",
                out.warn("!"),
                line.sku,
                line.errors.join("; ")
            )?;
        }
        super::write_count(out, c.lines.len(), "line", None)
    }
}

#[derive(Serialize)]
pub struct WishlistView<'a> {
    pub wishlist: &'a Wishlist,
    pub banner: String,
}

impl<'a> WishlistView<'a> {
    pub fn new(wishlist: &'a Wishlist, banner: bgnz_api::Banner) -> WishlistView<'a> {
        WishlistView {
            wishlist,
            banner: banner.name().to_string(),
        }
    }
}

impl View for WishlistView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let w = self.wishlist;
        if w.items.is_empty() {
            return writeln!(out, "The {} wishlist is empty.", self.banner);
        }
        let mut t = table(&["Item", "SKU", "Product", "Brand", "In stock"]);
        for i in &w.items {
            t.add_row(vec![
                i.id.clone(),
                i.sku.clone(),
                i.name.clone(),
                i.brand.clone().unwrap_or_else(|| "—".into()),
                super::yes_no(i.in_stock),
            ]);
        }
        writeln!(out, "{t}")?;
        super::write_count(
            out,
            w.items.len(),
            "item",
            Some("Remove one: `bgnz wishlist remove <item>`."),
        )
    }
}
