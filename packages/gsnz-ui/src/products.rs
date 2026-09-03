//! A product listing.
//!
//! Not a table. A product has a price, a unit price, a size, a stock state and
//! sometimes a multi-buy, and squeezing that into columns wastes most of the
//! width on empty cells. Grouping by title instead puts the size variants of
//! one product together rather than scattering them through the results.

use std::io::{self, Write};

use cli_kit::{plural, qualified, serde_json, Out, View};
use gsnz_core::{dollars, Product, RetailerId};
use serde::Serialize;

/// The price, with the "was" price alongside when something is on special.
pub fn price_label(p: &Product) -> String {
    let Some(now) = p.price_cents else {
        return "—".to_string();
    };
    let mut label = dollars(now);
    if let Some(saving) = p.saving_cents() {
        label.push_str(&format!(
            " (was {}, save {})",
            dollars(p.was_price_cents.unwrap_or(0)),
            dollars(saving)
        ));
    } else if p.is_special {
        // On special, but no "was" price was sent to compare against.
        label.push_str(" (special)");
    }
    if p.is_member_price {
        label.push_str(" [member price]");
    }
    label
}

pub fn unit_label(p: &Product) -> Option<String> {
    match (p.unit_price_cents, p.unit_measure.as_deref()) {
        (Some(c), Some(m)) => Some(format!("{} per {m}", dollars(c))),
        (Some(c), None) => Some(dollars(c)),
        _ => None,
    }
}

#[derive(Serialize)]
pub struct ProductList<'a> {
    pub products: &'a [Product],
    pub retailer: RetailerId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<&'a str>,
    /// What the retailer says exists, which is usually more than was asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
}

impl<'a> ProductList<'a> {
    pub fn new(products: &'a [Product], retailer: RetailerId) -> ProductList<'a> {
        ProductList {
            products,
            retailer,
            store: None,
            total: None,
        }
    }

    pub fn at(mut self, store: Option<&'a str>) -> ProductList<'a> {
        self.store = store;
        self
    }

    pub fn of(mut self, total: Option<u32>) -> ProductList<'a> {
        self.total = total;
        self
    }

    /// Products grouped by title, first-seen order preserved.
    fn groups(&self) -> Vec<(String, Vec<&'a Product>)> {
        let mut order: Vec<String> = Vec::new();
        let mut groups: std::collections::HashMap<String, Vec<&'a Product>> =
            std::collections::HashMap::new();
        for p in self.products {
            let key = p.title().to_lowercase();
            groups.entry(key.clone()).or_insert_with(|| {
                order.push(key.clone());
                Vec::new()
            });
            groups.get_mut(&key).expect("just inserted").push(p);
        }
        order
            .into_iter()
            .filter_map(|k| groups.remove(&k).map(|v| (k, v)))
            .collect()
    }
}

impl View for ProductList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.products.is_empty() {
            return writeln!(out, "Nothing found.");
        }
        let groups = self.groups();
        let heading = qualified(self.retailer.name(), self.store);
        writeln!(
            out,
            "{} — {} product{} in {} group{}\n",
            heading,
            self.products.len(),
            plural(self.products.len()),
            groups.len(),
            plural(groups.len()),
        )?;

        for (_, variants) in &groups {
            let title = variants[0].title();
            writeln!(out, "{}", out.heading(&title))?;
            for p in variants {
                let mut head = Vec::new();
                if let Some(size) = p.size.as_deref().filter(|s| !s.is_empty()) {
                    head.push(format!("Size: {size}"));
                }
                head.push(format!("Price: {}", price_label(p)));
                if let Some(mb) = &p.multi_buy {
                    head.push(format!("Multi-buy: {mb}"));
                }
                writeln!(out, "  - {}", head.join(" | "))?;

                let stock = match p.in_stock {
                    Some(true) => out.good("in stock"),
                    Some(false) => out.bad("unavailable"),
                    None => "stock unknown".to_string(),
                };
                let mut tail = vec![format!("SKU: {}", p.sku), stock];
                if let Some(unit) = unit_label(p) {
                    tail.push(unit);
                }
                if let Some(dept) = p.department.as_deref().filter(|d| !d.is_empty()) {
                    tail.push(dept.to_string());
                }
                writeln!(out, "    {}", tail.join(" | "))?;
            }
            writeln!(out)?;
        }
        Ok(())
    }

    /// The array alone, not the wrapper. A script asking for products wants to
    /// pipe them into `jq '.[0]'`, not reach through a `products` key.
    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self.products).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use gsnz_core::SaleUnit;

    fn product(name: &str, size: &str, cents: Option<i64>) -> Product {
        Product {
            retailer: RetailerId::NewWorld,
            sku: format!("{name}-{size}"),
            key: format!("{name}-{size}"),
            name: name.into(),
            brand: Some("Anchor".into()),
            size: Some(size.into()),
            price_cents: cents,
            was_price_cents: None,
            unit_price_cents: Some(225),
            unit_measure: Some("1L".into()),
            sale_unit: SaleUnit::Each,
            multi_buy: None,
            is_special: false,
            is_member_price: false,
            in_stock: Some(true),
            availability: None,
            department: None,
            image: None,
            url: None,
        }
    }

    fn render(list: &ProductList) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, list).unwrap();
        out.into_string()
    }

    #[test]
    fn prices_say_when_something_is_on_special() {
        let mut p = product("Blue Milk", "2L", Some(450));
        assert_eq!(price_label(&p), "$4.50");
        p.is_special = true;
        assert_eq!(price_label(&p), "$4.50 (special)");
        p.was_price_cents = Some(500);
        assert_eq!(price_label(&p), "$4.50 (was $5.00, save $0.50)");
        p.price_cents = None;
        assert_eq!(price_label(&p), "—");
    }

    #[test]
    fn a_member_price_is_labelled() {
        let mut p = product("Blue Milk", "2L", Some(450));
        p.is_member_price = true;
        assert!(price_label(&p).contains("[member price]"));
    }

    #[test]
    fn unit_pricing_needs_a_measure_to_be_useful() {
        let p = product("Blue Milk", "2L", Some(450));
        assert_eq!(unit_label(&p).as_deref(), Some("$2.25 per 1L"));
    }

    #[test]
    fn size_variants_of_one_product_are_grouped_together() {
        let products = vec![
            product("Blue Milk", "2L", Some(450)),
            product("Trim Milk", "2L", Some(440)),
            product("Blue Milk", "1L", Some(280)),
        ];
        let list = ProductList::new(&products, RetailerId::NewWorld);
        assert_eq!(list.groups().len(), 2, "two titles, three products");

        let text = render(&list);
        assert!(text.contains("3 products in 2 groups"), "{text}");
        // The group heading appears once even though there are two sizes.
        assert_eq!(text.matches("Anchor Blue Milk\n").count(), 1, "{text}");
    }

    #[test]
    fn stock_state_is_spelled_out_including_when_it_is_unknown() {
        let mut products = vec![product("A", "1L", Some(100))];
        products[0].in_stock = None;
        assert!(
            render(&ProductList::new(&products, RetailerId::NewWorld)).contains("stock unknown")
        );
        products[0].in_stock = Some(false);
        assert!(render(&ProductList::new(&products, RetailerId::NewWorld)).contains("unavailable"));
    }

    #[test]
    fn an_empty_result_says_so_rather_than_printing_a_bare_heading() {
        let text = render(&ProductList::new(&[], RetailerId::Woolworths));
        assert_eq!(text, "Nothing found.\n");
    }

    #[test]
    fn json_is_the_bare_array_so_it_pipes_into_jq() {
        let products = vec![product("Blue Milk", "2L", Some(450))];
        let mut out = Out::buffer(Format::Json);
        emit(&mut out, &ProductList::new(&products, RetailerId::NewWorld)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&out.into_string()).unwrap();
        assert!(value.is_array(), "got {value}");
        assert_eq!(value[0]["price"], 4.50, "money is dollars in json");
        // The same spelling `RetailerId::id` uses, which is also what config
        // keys and state directory names use. One machine name, not four.
        assert_eq!(value[0]["retailer"], "newworld");
    }
}
