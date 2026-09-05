//! One product, in full.

use std::io::{self, Write};

use cli_kit::{serde_json, table, Out, View};
use kmart_api::{Availability, Product};
use serde::Serialize;

#[derive(Serialize)]
pub struct ProductDetailView<'a> {
    pub product: &'a Product,
    pub country: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<&'a Availability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl View for ProductDetailView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let p = self.product;
        writeln!(out, "{}", out.heading(&p.name))?;

        let mut t = table(&["", ""]);
        t.add_row(vec!["Keycode".to_string(), p.keycode.clone()]);
        t.add_row(vec!["Price".to_string(), super::price_label(p)]);
        if let Some(currency) = &p.currency {
            t.add_row(vec!["Currency".to_string(), currency.clone()]);
        }
        t.add_row(vec![
            "Brand".to_string(),
            super::or_dash(p.brand.as_deref()),
        ]);
        t.add_row(vec![
            "Seller".to_string(),
            match p.marketplace() {
                true => format!("{} (marketplace)", super::or_dash(p.seller.as_deref())),
                false => super::or_dash(p.seller.as_deref()),
            },
        ]);
        if let Some(colour) = &p.colour {
            t.add_row(vec!["Colour".to_string(), colour.clone()]);
        }
        if let Some(size) = &p.size {
            t.add_row(vec!["Size".to_string(), size.clone()]);
        }
        if let Some(r) = p.rating {
            t.add_row(vec![
                "Rating".to_string(),
                format!("{:.1} from {} reviews", r.score, r.reviews),
            ]);
        }
        if p.free_shipping {
            t.add_row(vec!["Shipping".to_string(), "free".to_string()]);
        }
        if let Some(url) = &self.url {
            t.add_row(vec!["Page".to_string(), url.clone()]);
        }
        writeln!(out, "{t}")?;

        // Only when there is a choice to make. One variation is the product
        // itself wearing a different hat, and a table of one is noise.
        if p.variations.len() > 1 {
            let mut v = table(&["Keycode", "Variation", "Size", "Colour", "Price"]);
            for var in &p.variations {
                v.add_row(vec![
                    var.keycode.clone(),
                    super::or_dash(var.name.as_deref()),
                    super::or_dash(var.size.as_deref()),
                    super::or_dash(var.colour.as_deref()),
                    var.price.map(super::money).unwrap_or_else(|| "—".into()),
                ]);
            }
            writeln!(out, "{v}")?;
            // The keycode a person needs is the variation's, not the parent's.
            writeln!(
                out,
                "{}",
                out.dim(&format!(
                    "{} variations. Add one by its own keycode.",
                    p.variations.len()
                ))
            )?;
        }

        match self.availability {
            Some(a) => super::stock::write_summary(out, a)?,
            None if p.marketplace() => writeln!(
                out,
                "{}",
                out.dim("Sold by a marketplace seller, so it is in no store.")
            )?,
            None => {}
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
    use kmart_api::{Price, Rating, Variation};

    fn product() -> Product {
        Product {
            keycode: "43165537".into(),
            name: "Milk Frother - Black".into(),
            brand: Some("Anko".into()),
            price: Some(Price::cents(3900)),
            was: None,
            currency: Some("NZD".into()),
            colour: Some("Black".into()),
            size: None,
            seller: Some("Kmart".into()),
            rating: Some(Rating {
                score: 4.4,
                reviews: 306,
            }),
            url: Some("/product/milk-frother-black-43165537/".into()),
            image: None,
            images: Vec::new(),
            description: None,
            free_shipping: true,
            national_inventory: false,
            category_id: None,
            variations: Vec::new(),
            extra: Default::default(),
        }
    }

    fn render(view: &ProductDetailView<'_>) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, view).unwrap();
        out.into_string()
    }

    #[test]
    fn a_product_shows_what_a_person_would_want_before_buying() {
        let p = product();
        let text = render(&ProductDetailView {
            product: &p,
            country: "New Zealand",
            availability: None,
            url: Some("https://www.kmart.co.nz/product/x/".into()),
        });
        assert!(text.contains("Milk Frother - Black"), "{text}");
        assert!(text.contains("43165537"), "{text}");
        assert!(text.contains("$39.00"), "{text}");
        assert!(text.contains("4.4 from 306 reviews"), "{text}");
        assert!(text.contains("free"), "{text}");
    }

    #[test]
    fn one_variation_is_not_a_table_but_several_are() {
        let mut p = product();
        p.variations = vec![Variation {
            keycode: "43165537".into(),
            name: Some("Milk Frother - Black".into()),
            colour: None,
            size: None,
            price: None,
        }];
        let text = render(&ProductDetailView {
            product: &p,
            country: "New Zealand",
            availability: None,
            url: None,
        });
        assert!(!text.contains("variations."), "a table of one is noise");

        p.variations.push(Variation {
            keycode: "43691791".into(),
            name: Some("Milk Frother - Pink".into()),
            colour: Some("Pink".into()),
            size: None,
            price: Some(Price::cents(3900)),
        });
        let text = render(&ProductDetailView {
            product: &p,
            country: "New Zealand",
            availability: None,
            url: None,
        });
        assert!(text.contains("2 variations"), "{text}");
        // The keycode that goes in a cart is the variation's own.
        assert!(text.contains("43691791"), "{text}");
        assert!(text.contains("its own keycode"), "{text}");
    }

    #[test]
    fn a_marketplace_item_explains_why_there_is_no_stock_to_show() {
        let mut p = product();
        p.seller = Some("Some Other Seller".into());
        let text = render(&ProductDetailView {
            product: &p,
            country: "New Zealand",
            availability: None,
            url: None,
        });
        assert!(text.contains("marketplace"), "{text}");
        assert!(text.contains("in no store"), "{text}");
    }
}
