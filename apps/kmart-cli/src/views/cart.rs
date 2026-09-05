//! The shopping bag.

use std::io::{self, Write};

use cli_kit::{serde_json, table, Out, View};
use kmart_api::Cart;
use serde::Serialize;

#[derive(Serialize)]
pub struct CartView<'a> {
    pub cart: Option<&'a Cart>,
}

impl View for CartView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let Some(cart) = self.cart else {
            return writeln!(out, "The bag is empty.");
        };
        if cart.lines.is_empty() {
            return writeln!(out, "The bag is empty.");
        }

        let mut t = table(&["Keycode", "Product", "Qty", "Each", "Total"]);
        for line in &cart.lines {
            let name = match line.options.is_empty() {
                true => line.name.clone(),
                // Two sizes of the same shirt carry the same name, so without
                // these a bag with both in it reads as a duplicate.
                false => format!(
                    "{} ({})",
                    line.name,
                    line.options
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            t.add_row(vec![
                line.keycode.clone(),
                match &line.seller {
                    Some(seller) if !seller.eq_ignore_ascii_case("kmart") => {
                        format!("{name} [{seller}]")
                    }
                    _ => name,
                },
                line.quantity.to_string(),
                line.unit_price
                    .map(super::money)
                    .unwrap_or_else(|| "—".into()),
                line.total.map(super::money).unwrap_or_else(|| "—".into()),
            ]);
        }
        writeln!(out, "{t}")?;

        // Delivery is inside the total, so without it the sum does not appear
        // to add up: a $7 item in a $13 cart reads as a dropped line rather
        // than as postage.
        if let Some(shipping) = cart.shipping.filter(|s| s.cents > 0) {
            writeln!(out, "Subtotal {}", super::money(cart.subtotal()))?;
            match &cart.shipping_method {
                Some(method) => writeln!(out, "Delivery {} ({method})", super::money(shipping))?,
                None => writeln!(out, "Delivery {}", super::money(shipping))?,
            }
        }
        if let Some(total) = cart.total {
            writeln!(
                out,
                "{}",
                out.heading(&format!("Total {}", super::money(total)))
            )?;
        }
        super::write_count(out, cart.lines.len(), "line", None)
    }

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use kmart_api::{CartLine, Price};

    fn line(keycode: &str, name: &str, quantity: i64) -> CartLine {
        CartLine {
            id: format!("line-{keycode}"),
            keycode: keycode.into(),
            name: name.into(),
            quantity,
            unit_price: Some(Price::cents(3900)),
            total: Some(Price::cents(3900 * quantity)),
            seller: Some("Kmart".into()),
            options: Vec::new(),
        }
    }

    fn render(cart: Option<&Cart>) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &CartView { cart }).unwrap();
        out.into_string()
    }

    fn cart(lines: Vec<CartLine>) -> Cart {
        let total = lines.iter().filter_map(|l| l.total).map(|p| p.cents).sum();
        Cart {
            id: "cart-1".into(),
            version: 3,
            total: Some(Price::cents(total)),
            collect_store_id: None,
            shipping: None,
            shipping_method: None,
            lines,
        }
    }

    #[test]
    fn a_bag_shows_a_line_each_and_a_total() {
        let c = cart(vec![line("43165537", "Milk Frother", 2)]);
        let text = render(Some(&c));
        assert!(text.contains("43165537"), "{text}");
        assert!(text.contains("$78.00"), "{text}");
        assert!(text.contains("Total $78.00"), "{text}");
    }

    #[test]
    fn no_cart_and_an_empty_cart_read_the_same() {
        // A shopper who has never started one and one who emptied theirs are
        // in the same position, and should not be told different things.
        assert_eq!(render(None), "The bag is empty.\n");
        assert_eq!(render(Some(&cart(Vec::new()))), "The bag is empty.\n");
    }

    #[test]
    fn variant_options_are_shown_because_the_name_does_not_carry_them() {
        let mut l = line("43165537", "Plain Crew Neck T-shirt", 1);
        l.options = vec![("Size".into(), "L".into())];
        let text = render(Some(&cart(vec![l])));
        assert!(text.contains("Size: L"), "{text}");
    }

    #[test]
    fn delivery_is_broken_out_so_the_total_adds_up() {
        // The gateway folds postage into the total. Printing only the total
        // makes a $7 item in a $13 cart look like a dropped line.
        let mut c = cart(vec![line("43542444", "Mini Blocks", 1)]);
        c.shipping = Some(Price::cents(600));
        c.shipping_method = Some("Standard".into());
        c.total = Some(Price::cents(1300));
        let text = render(Some(&c));
        assert!(text.contains("Subtotal $39.00"), "{text}");
        assert!(text.contains("Delivery $6.00 (Standard)"), "{text}");
        assert!(text.contains("Total $13.00"), "{text}");
    }

    #[test]
    fn free_delivery_is_not_a_line_worth_printing() {
        let mut c = cart(vec![line("43542444", "Mini Blocks", 1)]);
        c.shipping = Some(Price::cents(0));
        let text = render(Some(&c));
        assert!(!text.contains("Delivery"), "{text}");
        assert!(!text.contains("Subtotal"), "{text}");
    }

    #[test]
    fn a_marketplace_line_names_its_seller() {
        let mut l = line("M1", "Third Party Thing", 1);
        l.seller = Some("Some Other Seller".into());
        let text = render(Some(&cart(vec![l])));
        assert!(text.contains("[Some Other Seller]"), "{text}");
    }
}
