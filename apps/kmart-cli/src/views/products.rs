//! A product listing, and the refinements it offers.

use std::io::{self, Write};

use cli_kit::{serde_json, table, Out, View};
use kmart_api::{Facet, Listing};
use serde::Serialize;

#[derive(Serialize)]
pub struct ProductList<'a> {
    pub listing: &'a Listing,
    pub country: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub island: Option<&'a str>,
}

impl<'a> ProductList<'a> {
    pub fn new(listing: &'a Listing, country: &'a str) -> ProductList<'a> {
        ProductList {
            listing,
            country,
            island: None,
        }
    }

    pub fn on(mut self, island: Option<&'a str>) -> ProductList<'a> {
        self.island = island;
        self
    }
}

impl View for ProductList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.listing.products.is_empty() {
            return writeln!(out, "Nothing found.");
        }

        // Said plainly rather than left to be inferred from odd results. The
        // index falls back to a semantic match, so a term matching nothing
        // still answers with twenty products -- and a person who typed a
        // keycode that no longer exists deserves to know that is what
        // happened.
        if self.listing.is_guess() {
            writeln!(
                out,
                "{}",
                out.dim("Nothing matched exactly; these are the closest things Kmart has.")
            )?;
        }

        let mut t = table(&["Keycode", "Product", "Brand", "Price"]);
        for p in &self.listing.products {
            t.add_row(vec![
                p.keycode.clone(),
                // Marked because it ships separately and is in no store, so
                // `stock` has nothing useful to say about it.
                if p.marketplace() {
                    format!("{} [marketplace]", p.name)
                } else {
                    p.name.clone()
                },
                super::or_dash(p.brand.as_deref()),
                super::price_label(p),
            ]);
        }
        writeln!(out, "{t}")?;

        let shown = self.listing.products.len();
        let next = match self.listing.total as usize > shown {
            true => Some(format!(
                "{} in all; raise --limit or use --page for more.",
                self.listing.total
            )),
            false => None,
        };
        super::write_count(out, shown, "product", next.as_deref())?;

        let mut note = format!("Prices in {}", self.country);
        if let Some(island) = self.island {
            note.push_str(&format!(", ranged for the {island} island"));
        }
        writeln!(out, "{}", out.dim(&format!("{note}.")))?;
        Ok(())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// The refinements a listing offers, for `--facets`.
///
/// Its own view because Kmart's facets differ per category -- there is no
/// fixed list to put in `--help`, so the way to find out is to ask.
#[derive(Serialize)]
pub struct FacetList<'a> {
    pub facets: &'a [Facet],
}

impl View for FacetList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.facets.is_empty() {
            return writeln!(out, "This listing offers no refinements.");
        }
        let mut t = table(&["Facet", "Values"]);
        for f in self.facets {
            let values: Vec<String> = f
                .options
                .iter()
                .map(|o| match o.count {
                    Some(n) => format!("{} ({n})", o.value),
                    None => o.value.clone(),
                })
                .collect();
            t.add_row(vec![f.name.clone(), values.join(", ")]);
        }
        writeln!(out, "{t}")?;
        super::write_count(
            out,
            self.facets.len(),
            "refinement",
            Some("Use --filter 'NAME=VALUE'."),
        )
    }

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use kmart_api::{FacetOption, Price, Product};

    fn product(keycode: &str, name: &str) -> Product {
        Product {
            keycode: keycode.into(),
            name: name.into(),
            brand: Some("Anko".into()),
            price: Some(Price::cents(3900)),
            was: None,
            currency: Some("NZD".into()),
            colour: None,
            size: None,
            seller: Some("Kmart".into()),
            rating: None,
            url: None,
            image: None,
            images: Vec::new(),
            description: None,
            free_shipping: false,
            national_inventory: false,
            category_id: None,
            variations: Vec::new(),
            extra: Default::default(),
        }
    }

    fn listing(products: Vec<Product>, total: u64, exact: u64) -> Listing {
        Listing {
            products,
            total,
            exact: Some(exact),
            facets: Vec::new(),
            sorts: Vec::new(),
        }
    }

    fn render(view: &ProductList<'_>) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, view).unwrap();
        out.into_string()
    }

    #[test]
    fn a_listing_renders_a_row_per_product_and_a_count() {
        let l = listing(vec![product("1", "First"), product("2", "Second")], 2, 2);
        let text = render(&ProductList::new(&l, "New Zealand"));
        assert!(text.contains("First"), "{text}");
        assert!(text.contains("$39.00"), "{text}");
        assert!(text.contains("2 products."), "{text}");
        assert!(!text.contains("raise --limit"), "nothing more to fetch");
        assert!(text.contains("Prices in New Zealand"), "{text}");
    }

    #[test]
    fn a_truncated_listing_says_how_to_see_the_rest() {
        let l = listing(vec![product("1", "First")], 3122, 3122);
        let text = render(&ProductList::new(&l, "Australia"));
        assert!(text.contains("3122 in all; raise --limit"), "{text}");
    }

    #[test]
    fn a_semantic_fallback_says_so_rather_than_passing_itself_off() {
        // Nothing matched the words typed; these are the index guessing.
        let l = listing(vec![product("1", "Something Else")], 20, 0);
        let text = render(&ProductList::new(&l, "New Zealand"));
        assert!(text.contains("Nothing matched exactly"), "{text}");
    }

    #[test]
    fn an_empty_listing_says_so_instead_of_printing_an_empty_table() {
        let l = listing(Vec::new(), 0, 0);
        assert_eq!(
            render(&ProductList::new(&l, "New Zealand")),
            "Nothing found.\n"
        );
    }

    #[test]
    fn a_reduced_price_shows_what_it_was() {
        let mut p = product("1", "Reduced Thing");
        p.was = Some(Price::cents(5900));
        let l = listing(vec![p], 1, 1);
        let text = render(&ProductList::new(&l, "New Zealand"));
        assert!(text.contains("$39.00 (was $59.00)"), "{text}");
    }

    #[test]
    fn a_marketplace_item_is_marked_because_stock_cannot_speak_for_it() {
        let mut p = product("M1", "Third Party Thing");
        p.seller = Some("Some Other Seller".into());
        let l = listing(vec![p], 1, 1);
        let text = render(&ProductList::new(&l, "New Zealand"));
        assert!(text.contains("[marketplace]"), "{text}");
    }

    #[test]
    fn the_island_is_named_when_one_was_used() {
        let l = listing(vec![product("1", "First")], 1, 1);
        let text = render(&ProductList::new(&l, "New Zealand").on(Some("SI")));
        assert!(text.contains("ranged for the SI island"), "{text}");
    }

    #[test]
    fn facets_are_listed_with_their_counts_because_they_differ_per_category() {
        let facets = vec![Facet {
            name: "Power Rating".into(),
            label: None,
            options: vec![FacetOption {
                value: "1000 to 1500W".into(),
                label: None,
                count: Some(3),
            }],
        }];
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &FacetList { facets: &facets }).unwrap();
        let text = out.into_string();
        assert!(text.contains("Power Rating"), "{text}");
        assert!(text.contains("1000 to 1500W (3)"), "{text}");
        assert!(text.contains("--filter"), "{text}");
    }

    #[test]
    fn json_and_text_come_from_the_same_struct() {
        let l = listing(vec![product("43165537", "Milk Frother")], 2, 2);
        let mut out = Out::buffer(Format::Json);
        emit(&mut out, &ProductList::new(&l, "New Zealand")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&out.into_string()).unwrap();
        assert_eq!(value["listing"]["products"][0]["keycode"], "43165537");
        assert_eq!(value["listing"]["total"], 2);
        // Absent rather than null: a script checking for an island should not
        // have to tell the two apart.
        assert!(value.get("island").is_none());
    }
}
