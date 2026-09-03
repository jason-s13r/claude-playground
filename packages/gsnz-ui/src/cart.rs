//! The cart, and the money underneath it.

use std::io::{self, Write};

use cli_kit::comfy_table::Cell;
use cli_kit::{qualified, table, Out, View};
use gsnz_core::{dollars, Cart};
use serde::Serialize;

#[derive(Serialize)]
#[serde(transparent)]
pub struct CartView<'a>(pub &'a Cart);

impl View for CartView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let cart = self.0;
        let store = cart.store.as_ref().and_then(|s| s.name.as_deref());
        let heading = qualified(cart.retailer.name(), store);

        if cart.is_empty() {
            return writeln!(out, "{heading}\n\nThe cart is empty.");
        }
        writeln!(out, "{heading}\n")?;

        let mut t = table(&["Qty", "Product", "SKU", "Line total"]);
        for line in &cart.lines {
            t.add_row(vec![
                Cell::new(line.quantity.format()),
                Cell::new(&line.name),
                Cell::new(&line.sku),
                Cell::new(line.total_cents.map(dollars).unwrap_or_else(|| "—".into())),
            ]);
        }
        writeln!(out, "{t}")?;

        // Subtotal, then whatever the retailer added or took off, then the
        // total. Adjustments are printed as they arrive rather than being
        // matched against a fixed set of known fees.
        if let Some(subtotal) = cart.subtotal_cents {
            writeln!(out, "  {:<24}{:>10}", "Subtotal", dollars(subtotal))?;
        }
        for adjustment in &cart.adjustments {
            writeln!(
                out,
                "  {:<24}{:>10}",
                adjustment.label,
                dollars(adjustment.cents)
            )?;
        }
        if let Some(total) = cart.total_cents {
            writeln!(out, "  {:<24}{:>10}", "Total", dollars(total))?;
        }

        if !cart.unavailable.is_empty() {
            writeln!(out, "\n{}", out.warn("Unavailable at this store:"))?;
            for line in &cart.unavailable {
                writeln!(
                    out,
                    "  {} {} ({})",
                    line.quantity.format(),
                    line.name,
                    line.sku
                )?;
            }
        }
        for note in &cart.notes {
            writeln!(out, "\n{}", out.warn(note))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use gsnz_core::{Adjustment, CartLine, Quantity, RetailerId, StoreRef};

    fn line(name: &str, quantity: Quantity, cents: i64) -> CartLine {
        CartLine {
            key: format!("{name}-EA"),
            sku: name.into(),
            name: name.into(),
            brand: None,
            quantity,
            unit_price_cents: Some(cents),
            total_cents: Some(cents),
        }
    }

    fn cart(lines: Vec<CartLine>) -> Cart {
        Cart {
            retailer: RetailerId::Woolworths,
            store: Some(StoreRef {
                id: "4123".into(),
                name: Some("Woolworths Regent".into()),
            }),
            lines,
            unavailable: Vec::new(),
            subtotal_cents: Some(1000),
            total_cents: Some(1150),
            adjustments: vec![Adjustment {
                label: "Service fee".into(),
                cents: 150,
            }],
            member: None,
            fulfilment: None,
            notes: Vec::new(),
            priced_at: None,
        }
    }

    fn render(cart: &Cart) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &CartView(cart)).unwrap();
        out.into_string()
    }

    #[test]
    fn an_empty_cart_says_so_and_prints_no_table() {
        let mut c = cart(Vec::new());
        c.lines.clear();
        let text = render(&c);
        assert!(text.contains("The cart is empty."), "{text}");
        assert!(!text.contains("Line total"), "no table: {text}");
    }

    #[test]
    fn adjustments_are_printed_as_they_arrive() {
        let text = render(&cart(vec![line("Milk", Quantity::units(2), 500)]));
        assert!(text.contains("Subtotal"), "{text}");
        assert!(text.contains("Service fee"), "{text}");
        assert!(text.contains("$1.50"), "{text}");
        assert!(text.contains("Total"), "{text}");
    }

    #[test]
    fn a_weight_line_prints_its_kilograms() {
        let text = render(&cart(vec![line("Bananas", Quantity::kilograms(1.5), 450)]));
        assert!(text.contains("1.5kg"), "{text}");
    }

    #[test]
    fn the_store_name_is_not_prefixed_with_a_chain_it_already_carries() {
        let text = render(&cart(vec![line("Milk", Quantity::units(1), 500)]));
        assert!(text.contains("Woolworths Regent"), "{text}");
        assert!(!text.contains("Woolworths — Woolworths"), "{text}");
    }

    #[test]
    fn unavailable_lines_are_listed_apart_from_the_totals() {
        let mut c = cart(vec![line("Milk", Quantity::units(1), 500)]);
        c.unavailable.push(line("Bread", Quantity::units(1), 400));
        let text = render(&c);
        assert!(text.contains("Unavailable at this store"), "{text}");
        assert!(text.contains("Bread"), "{text}");
    }

    #[test]
    fn json_is_the_cart_itself_with_money_in_dollars() {
        let mut out = Out::buffer(Format::Json);
        emit(
            &mut out,
            &CartView(&cart(vec![line("Milk", Quantity::units(2), 500)])),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&out.into_string()).unwrap();
        assert_eq!(value["subtotal"], 10.0);
        assert_eq!(value["lines"][0]["quantity"]["unit"], "units");
        assert_eq!(value["lines"][0]["quantity"]["count"], 2);
    }
}
