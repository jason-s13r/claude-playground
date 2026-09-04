//! One product in full.
//!
//! Not a table: a detail page has a description, a price, several variation
//! axes and two kinds of availability, and none of that is columnar. The
//! variation axes are the part worth laying out carefully, because a value can
//! be unavailable for two different reasons and the difference is what tells
//! someone whether to pick another size or another colour.

use std::io::{self, Write};

use cli_kit::{table, Out, View};
use serde::Serialize;
use twlnz_api::ProductDetail;

#[derive(Serialize)]
pub struct ProductDetailView<'a> {
    #[serde(flatten)]
    pub detail: &'a ProductDetail,
}

impl<'a> ProductDetailView<'a> {
    pub fn new(detail: &'a ProductDetail) -> ProductDetailView<'a> {
        ProductDetailView { detail }
    }
}

impl View for ProductDetailView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let p = &self.detail.product;
        writeln!(out, "{}", out.heading(&p.name))?;

        let mut line = super::price_label(p);
        line.push_str(&format!("  {}", super::stock_colored(out, &p.availability)));
        writeln!(out, "{line}")?;
        // The site's own phrasing, which says more than the two booleans can:
        // "In-store only" is clearer than "online: no, in store: yes".
        if let Some(label) = &p.availability.label {
            writeln!(out, "{}", out.dim(label))?;
        }

        writeln!(out)?;
        field(out, "id", &p.id)?;
        if p.is_variant() {
            // Worth saying: the price and stock above are for this one variant,
            // not for the family.
            if let Some(master) = &p.master_id {
                field(out, "variant of", master)?;
            }
        }
        if let Some(brand) = &p.brand {
            field(out, "brand", brand)?;
        }
        if let Some(ean) = &p.ean {
            field(out, "barcode", ean)?;
        }
        if let Some(category) = &p.category {
            field(out, "category", category)?;
        }
        if let Some(max) = self.detail.max_quantity {
            field(out, "max per order", &max.to_string())?;
        }
        if p.marketplace {
            field(out, "sold by", "a marketplace seller; ships separately")?;
        }

        if let Some(description) = &self.detail.description {
            writeln!(out)?;
            writeln!(out, "{description}")?;
        }

        for axis in &self.detail.axes {
            writeln!(out)?;
            let heading = match &axis.selected {
                Some(selected) => format!("{} ({selected})", axis.name),
                None => axis.name.clone(),
            };
            writeln!(out, "{}", out.heading(&heading))?;
            for value in &axis.values {
                // Three states, not two. "Does not exist in this colour" and
                // "exists but is sold out" send someone in different
                // directions.
                let marker = match (value.selected, value.selectable, value.orderable) {
                    (true, _, _) => out.good("*"),
                    (_, true, true) => " ".to_string(),
                    (_, true, false) => out.bad("x"),
                    (_, false, _) => out.dim("-"),
                };
                let note = match (value.selectable, value.orderable) {
                    (true, false) => out.bad(" sold out"),
                    (false, _) => out.dim(" not made in this combination"),
                    _ => String::new(),
                };
                writeln!(out, "  {marker} {}{note}", value.label)?;
            }
        }

        if !self.detail.shipping.is_empty() {
            writeln!(out)?;
            writeln!(out, "{}", out.heading("Delivery"))?;
            let mut t = table(&["Option", "When"]);
            for option in &self.detail.shipping {
                t.add_row(vec![
                    if option.pickup {
                        format!("{} (pick up)", option.name)
                    } else {
                        option.name.clone()
                    },
                    option.estimate.clone().unwrap_or_else(|| "—".into()),
                ]);
            }
            writeln!(out, "{t}")?;
        }
        Ok(())
    }
}

const LABEL: usize = 14;

fn field(out: &mut Out, label: &str, value: &str) -> io::Result<()> {
    writeln!(out, "{label:<LABEL$} {value}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use twlnz_api::{Availability, Price, Product, VariationAxis, VariationValue};

    fn detail() -> ProductDetail {
        ProductDetail {
            product: Product {
                id: "RM1-8M".into(),
                master_id: Some("RM1".into()),
                name: "Plain Cotton Tee".into(),
                brand: Some("Example Brand".into()),
                price: Price::from_display("$12.00"),
                availability: Availability {
                    status: Some("FIND_IN_STORE".into()),
                    online: Some(false),
                    in_store: Some(true),
                    label: Some("In-store only".into()),
                },
                ..Product::default()
            },
            description: Some("A tee.".into()),
            sku: Some("RM1-8M".into()),
            max_quantity: Some(10),
            axes: vec![VariationAxis {
                id: "size".into(),
                name: "Size".into(),
                selected: Some("M".into()),
                values: vec![
                    VariationValue {
                        id: "XS".into(),
                        label: "XS".into(),
                        selected: false,
                        selectable: false,
                        orderable: false,
                        url: None,
                    },
                    VariationValue {
                        id: "S".into(),
                        label: "S".into(),
                        selected: false,
                        selectable: true,
                        orderable: false,
                        url: None,
                    },
                    VariationValue {
                        id: "M".into(),
                        label: "M".into(),
                        selected: true,
                        selectable: true,
                        orderable: true,
                        url: None,
                    },
                ],
            }],
            shipping: vec![],
        }
    }

    fn render(detail: &ProductDetail) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &ProductDetailView::new(detail)).unwrap();
        out.into_string()
    }

    #[test]
    fn the_two_reasons_a_size_is_unavailable_are_told_apart() {
        // The whole point of rendering three states: one says pick another
        // size, the other says this combination was never made.
        let text = render(&detail());
        assert!(text.contains("x S sold out"), "{text}");
        assert!(text.contains("- XS not made in this combination"), "{text}");
        assert!(text.contains("* M"), "the chosen value is marked: {text}");
    }

    #[test]
    fn an_in_store_only_product_says_so_in_the_sites_own_words() {
        let text = render(&detail());
        let headline = text.lines().nth(1).unwrap_or_default();
        // The headline is about the product, and this product can be had.
        // "sold out" appears further down against the one size that is.
        assert!(headline.contains("in store"), "{headline}");
        assert!(!headline.contains("sold out"), "{headline}");
        assert!(text.contains("In-store only"), "{text}");
    }

    #[test]
    fn a_variant_names_the_family_it_belongs_to() {
        // Otherwise the price above reads as the price of every colour.
        let text = render(&detail());
        let line = text
            .lines()
            .find(|l| l.starts_with("variant of"))
            .unwrap_or_default();
        assert!(line.ends_with("RM1"), "{text}");
    }

    #[test]
    fn a_product_that_is_not_a_variant_does_not_claim_to_be_one() {
        let mut d = detail();
        d.product.master_id = Some(d.product.id.clone());
        d.axes.clear();
        let text = render(&d);
        assert!(!text.contains("variant of"), "{text}");
    }
}
