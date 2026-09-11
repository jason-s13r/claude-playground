//! A page of search or browse results.

use std::io::{self, Write};

use bgnz_api::Listing;
use cli_kit::{table, Out, View};
use serde::Serialize;

#[derive(Serialize)]
pub struct ProductList<'a> {
    pub listing: &'a Listing,
    /// Whether to print the facets the index offered.
    #[serde(skip)]
    pub facets: bool,
}

impl<'a> ProductList<'a> {
    pub fn new(listing: &'a Listing, facets: bool) -> ProductList<'a> {
        ProductList { listing, facets }
    }
}

impl View for ProductList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let l = self.listing;
        if l.products.is_empty() {
            return writeln!(out, "Nothing found at {}.", l.banner);
        }
        let mut t = table(&["SKU", "Product", "Brand", "Price", "In stock"]);
        for p in &l.products {
            t.add_row(vec![
                p.sku.clone(),
                p.name.clone(),
                p.brand.clone().unwrap_or_else(|| "—".into()),
                super::price_label(p.price, p.was_price),
                super::yes_no(p.in_stock),
            ]);
        }
        writeln!(out, "{t}")?;

        if self.facets && !l.facets.is_empty() {
            writeln!(out)?;
            for facet in &l.facets {
                let options: Vec<String> = facet
                    .options
                    .iter()
                    .take(8)
                    .map(|o| match o.count {
                        Some(n) => format!("{} ({n})", o.value),
                        None => o.value.clone(),
                    })
                    .collect();
                if !options.is_empty() {
                    writeln!(out, "{}: {}", out.heading(&facet.label), options.join(", "))?;
                }
            }
            writeln!(out)?;
        }

        // The window, not just the count: a listing is one page of many and
        // saying "36 products" of 173 would be a lie by omission.
        let shown = l.products.len() as u64;
        let last = l.offset + shown;
        writeln!(
            out,
            "{}–{} of {} at {}.{}",
            l.offset + 1,
            last,
            l.total,
            l.banner,
            if last < l.total {
                format!(" Next: --offset {last}")
            } else {
                String::new()
            }
        )
    }
}
