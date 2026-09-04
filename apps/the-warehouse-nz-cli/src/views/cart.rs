//! The basket.

use std::io::{self, Write};

use cli_kit::{table, Out, View};
use serde::Serialize;
use twlnz_api::Cart;

#[derive(Serialize)]
pub struct CartView<'a> {
    #[serde(flatten)]
    pub cart: &'a Cart,
    /// What just happened, when this is shown after a change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl<'a> CartView<'a> {
    pub fn new(cart: &'a Cart) -> CartView<'a> {
        CartView { cart, note: None }
    }

    pub fn after(mut self, note: impl Into<String>) -> CartView<'a> {
        self.note = Some(note.into());
        self
    }
}

impl View for CartView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if let Some(note) = &self.note {
            writeln!(out, "{note}")?;
        }
        if self.cart.lines.is_empty() {
            return writeln!(out, "The cart is empty.");
        }

        // Two columns, because the site sends one and derives the other, and a
        // single "Price" would mean the unit after `add` and the line after
        // `list`.
        let mut t = table(&["ID", "Product", "Qty", "Each", "Total"]);
        for line in &self.cart.lines {
            t.add_row(vec![
                line.id.clone(),
                line.name.clone(),
                line.quantity.to_string(),
                line.price.label().unwrap_or_else(|| "—".into()),
                line.total.label().unwrap_or_else(|| "—".into()),
            ]);
        }
        writeln!(out, "{t}")?;

        // Units, not lines: the same product added twice is two lines, and the
        // site counts what is in the basket.
        let unit = if self.cart.quantity == 1 {
            "item"
        } else {
            "items"
        };
        match &self.cart.subtotal {
            Some(subtotal) => writeln!(out, "{} {unit}, {subtotal}.", self.cart.quantity),
            None => writeln!(out, "{} {unit}.", self.cart.quantity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use twlnz_api::{CartLine, Price};

    fn cart() -> Cart {
        Cart {
            id: Some("c1".into()),
            lines: vec![CartLine {
                uuid: "u1".into(),
                pli_uuid: Some("p1".into()),
                id: "R1".into(),
                name: "A Thing".into(),
                brand: None,
                quantity: 2,
                price: Price::from_display("$7.49"),
                total: Price::from_display("$14.98"),
            }],
            subtotal: Some("$14.98".into()),
            quantity: 2,
        }
    }

    fn render(view: &CartView<'_>) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, view).unwrap();
        out.into_string()
    }

    #[test]
    fn a_cart_counts_units_rather_than_lines() {
        let cart = cart();
        let text = render(&CartView::new(&cart));
        assert!(text.contains("A Thing"), "{text}");
        assert!(text.contains("$7.49"), "each: {text}");
        assert!(text.contains("$14.98"), "line total: {text}");
        assert!(text.contains("2 items, $14.98."), "{text}");
    }

    #[test]
    fn a_change_says_what_it_did_before_showing_the_result() {
        let cart = cart();
        let text = render(&CartView::new(&cart).after("Added R1."));
        assert!(text.starts_with("Added R1."), "{text}");
    }

    #[test]
    fn an_empty_cart_says_so() {
        let cart = Cart::default();
        assert_eq!(render(&CartView::new(&cart)), "The cart is empty.\n");
    }
}
