//! One product page.

use std::io::{self, Write};

use bgnz_api::ProductDetail;
use cli_kit::{table, Out, View};
use serde::Serialize;

#[derive(Serialize)]
pub struct ProductView<'a> {
    pub product: &'a ProductDetail,
    pub banner: String,
}

impl<'a> ProductView<'a> {
    pub fn new(product: &'a ProductDetail, banner: bgnz_api::Banner) -> ProductView<'a> {
        ProductView {
            product,
            banner: banner.name().to_string(),
        }
    }
}

impl View for ProductView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let p = self.product;
        writeln!(out, "{}", out.heading(&p.name))?;

        // A headless two-column table for the facts, the same as the other
        // tools' product pages, so a person reading two of them side by side
        // is reading one layout.
        let mut t = table(&["", ""]);
        t.add_row(vec!["SKU".to_string(), p.sku.clone()]);
        t.add_row(vec![
            "Price".to_string(),
            super::price_label(
                p.price.as_ref().map(|m| m.value),
                p.was_price.as_ref().map(|m| m.value),
            ),
        ]);
        t.add_row(vec![
            "Stock".to_string(),
            match p.in_stock {
                Some(true) => "in stock".into(),
                Some(false) => "out of stock".into(),
                None => "—".into(),
            },
        ]);
        t.add_row(vec![
            "Brand".to_string(),
            p.brand.clone().unwrap_or_else(|| "—".into()),
        ]);
        t.add_row(vec!["Fascia".to_string(), self.banner.clone()]);
        writeln!(out, "{t}")?;

        if !p.categories.is_empty() {
            // Every category the product is merchandised into, not a
            // breadcrumb -- a jug is in forty of them, including "Sale backup"
            // and "Mens - DO NOT USE!". A handful is all a person wants, and
            // `--json` still carries the lot.
            const SHOWN: usize = 6;
            let mut line = p.categories.iter().take(SHOWN).cloned().collect::<Vec<_>>();
            if p.categories.len() > SHOWN {
                line.push(format!("+{} more", p.categories.len() - SHOWN));
            }
            writeln!(out, "{}", out.dim(&line.join(" · ")))?;
        }

        // Only when there is a choice to make. A simple product has none, and
        // a table of one is noise.
        if p.variants.len() > 1 {
            writeln!(out)?;
            let mut t = table(&["SKU", "Variant", "In stock", "Barcode"]);
            for v in &p.variants {
                t.add_row(vec![
                    v.sku.clone(),
                    v.label(),
                    super::yes_no(v.in_stock),
                    // Shown because it is what decides whether `bgnz stock` can
                    // answer for this variant at all.
                    v.fulfilment.barcode.clone().unwrap_or_else(|| "—".into()),
                ]);
            }
            writeln!(out, "{t}")?;
        }

        if !p.attributes.is_empty() {
            writeln!(out)?;
            let mut t = table(&["Spec", "Value"]);
            for a in &p.attributes {
                t.add_row(vec![a.name.clone(), a.value.clone()]);
            }
            writeln!(out, "{t}")?;
        }
        Ok(())
    }
}
