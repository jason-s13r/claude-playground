//! A product listing.
//!
//! A table, unlike the grocery equivalent. General merchandise has no unit
//! price and no size axis to group by, so the columns are full rather than
//! mostly empty, and a person scanning a search result wants to compare prices
//! down a column.

use std::io::{self, Write};

use cli_kit::{serde_json, table, Out, View};
use serde::Serialize;
use twlnz_api::Product;

#[derive(Serialize)]
pub struct ProductList<'a> {
    pub products: &'a [Product],
    /// What the site says exists, which is usually far more than was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    /// The category a keyword search landed in, when it redirected into one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub island: Option<String>,
}

impl<'a> ProductList<'a> {
    pub fn new(products: &'a [Product]) -> ProductList<'a> {
        ProductList {
            products,
            total: None,
            category: None,
            island: None,
        }
    }

    pub fn of(mut self, total: Option<u32>) -> ProductList<'a> {
        self.total = total;
        self
    }

    pub fn in_category(mut self, category: Option<&'a str>) -> ProductList<'a> {
        self.category = category;
        self
    }

    pub fn on(mut self, island: Option<twlnz_api::Island>) -> ProductList<'a> {
        self.island = island.map(|i| format!("{i} island"));
        self
    }
}

impl View for ProductList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.products.is_empty() {
            return writeln!(out, "Nothing found.");
        }

        // Told plainly rather than left to be inferred from odd results: a
        // keyword that resolved to a category is showing a department, not a
        // search.
        if let Some(category) = self.category {
            writeln!(
                out,
                "{}",
                out.dim(&format!("The search matched the department {category}."))
            )?;
        }

        let mut t = table(&["ID", "Product", "Brand", "Price", "Stock"]);
        for p in self.products {
            t.add_row(vec![
                p.id.clone(),
                // Marked because it ships separately and is not returnable to a
                // store, which is worth knowing before adding it to a cart.
                if p.marketplace {
                    format!("{} [marketplace]", p.name)
                } else {
                    p.name.clone()
                },
                p.brand.clone().unwrap_or_else(|| "—".into()),
                super::price_label(p),
                super::stock_label(&p.availability).to_string(),
            ]);
        }
        writeln!(out, "{t}")?;

        let shown = self.products.len();
        let next = match self.total {
            Some(total) if total as usize > shown => {
                Some(format!("{total} in all; raise --limit for more."))
            }
            _ => None,
        };
        super::write_count(out, shown, "product", next.as_deref())?;
        if let Some(island) = &self.island {
            writeln!(
                out,
                "{}",
                out.dim(&format!("Stock shown for the {island}."))
            )?;
        }
        Ok(())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use twlnz_api::{Availability, Price};

    fn product(id: &str, name: &str) -> Product {
        Product {
            id: id.into(),
            name: name.into(),
            brand: Some("Example Brand".into()),
            price: Price::from_display("$12.00"),
            availability: Availability {
                online: Some(true),
                ..Availability::default()
            },
            ..Product::default()
        }
    }

    fn render(list: &ProductList<'_>) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, list).unwrap();
        out.into_string()
    }

    #[test]
    fn a_listing_renders_a_row_per_product_and_a_count() {
        let products = vec![product("R1", "First Thing"), product("R2", "Second Thing")];
        let text = render(&ProductList::new(&products).of(Some(2)));
        assert!(text.contains("First Thing"), "{text}");
        assert!(text.contains("$12.00"), "{text}");
        assert!(text.contains("2 products."), "{text}");
        assert!(!text.contains("raise --limit"), "nothing more to fetch");
    }

    #[test]
    fn a_truncated_listing_says_how_to_see_the_rest() {
        let products = vec![product("R1", "First Thing")];
        let text = render(&ProductList::new(&products).of(Some(3122)));
        assert!(text.contains("3122 in all; raise --limit"), "{text}");
    }

    #[test]
    fn an_empty_listing_says_so_instead_of_printing_an_empty_table() {
        assert_eq!(render(&ProductList::new(&[])), "Nothing found.\n");
    }

    #[test]
    fn a_search_that_became_a_department_says_which_one() {
        let products = vec![product("R1", "Brick Set")];
        let text =
            render(&ProductList::new(&products).in_category(Some("toys-baby/top-brands/lego")));
        assert!(
            text.contains("matched the department toys-baby/top-brands/lego"),
            "{text}"
        );
    }

    #[test]
    fn a_marketplace_item_is_marked_because_it_ships_separately() {
        let mut p = product("M1", "Third Party Thing");
        p.marketplace = true;
        let text = render(&ProductList::new(std::slice::from_ref(&p)));
        assert!(text.contains("[marketplace]"), "{text}");
    }

    #[test]
    fn json_and_text_come_from_the_same_struct() {
        let products = vec![product("R1", "First Thing")];
        let mut out = Out::buffer(Format::Json);
        emit(&mut out, &ProductList::new(&products).of(Some(2))).unwrap();
        let value: serde_json::Value = serde_json::from_str(&out.into_string()).unwrap();
        assert_eq!(value["products"][0]["id"], "R1");
        assert_eq!(value["total"], 2);
        // Absent rather than null: a script checking for a category should not
        // have to tell the two apart.
        assert!(value.get("category").is_none());
    }
}
