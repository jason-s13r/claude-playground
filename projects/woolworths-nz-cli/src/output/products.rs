//! Rendering a product listing.

use owo_colors::{OwoColorize, Stream};

use crate::domain::{dollars, Product};
use crate::output::{plural, store_heading};

/// The price, with the "was" price alongside when something is on special.
pub(super) fn price_label(p: &Product) -> String {
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
        // On special, but the API sent no "was" price to compare against.
        label.push_str(" (special)");
    }
    if p.is_club_price {
        label.push_str(" [member price]");
    }
    label
}

pub(super) fn unit_label(p: &Product) -> Option<String> {
    match (p.unit_price_cents, p.unit_measure.as_deref()) {
        (Some(c), Some(m)) => Some(format!("{} per {m}", dollars(c))),
        (Some(c), None) => Some(dollars(c)),
        _ => None,
    }
}

/// Grouped by brand and name, so the size variants of one product sit together
/// rather than scattered through the results.
pub fn print_products(products: &[Product], store: Option<&str>) {
    if products.is_empty() {
        return;
    }

    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<&Product>> =
        std::collections::HashMap::new();
    for p in products {
        let key = p.title().to_lowercase();
        groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Vec::new()
        });
        groups.get_mut(&key).expect("just inserted").push(p);
    }

    println!(
        "{} — {} product{} in {} group{}\n",
        store_heading(store),
        products.len(),
        plural(products.len()),
        order.len(),
        plural(order.len()),
    );

    for key in &order {
        let variants = &groups[key];
        let title = variants[0].title();
        let heading = owo_colors::Style::new().cyan().bold();
        println!(
            "{}",
            title.if_supports_color(Stream::Stdout, |t| t.style(heading))
        );

        for p in variants {
            let mut head = vec![format!("Price: {}", price_label(p))];
            if let Some(unit) = unit_label(p) {
                head.push(unit);
            }
            if p.sponsored {
                head.push("promoted".to_string());
            }
            println!("  - {}", head.join(" | "));

            let stock = match p.in_stock {
                Some(true) => "in stock"
                    .if_supports_color(Stream::Stdout, |t| t.green())
                    .to_string(),
                Some(false) => "unavailable"
                    .if_supports_color(Stream::Stdout, |t| t.red())
                    .to_string(),
                None => "stock unknown".to_string(),
            };
            let mut tail = vec![format!("SKU: {}", p.sku), stock];
            if p.by_weight() {
                tail.push("sold by weight".to_string());
            }
            if let Some(dept) = p.department.as_deref().filter(|d| !d.is_empty()) {
                tail.push(dept.to_string());
            }
            println!("    {}", tail.join(" | "));
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(cents: Option<i64>) -> Product {
        Product {
            sku: "282768".into(),
            variant_key: "282768-EA".into(),
            name: "Milk Standard 3L".into(),
            brand: Some("Woolworths".into()),
            unit_of_measure: Some("EACH".into()),
            price_cents: cents,
            was_price_cents: None,
            unit_price_cents: Some(240),
            unit_measure: Some("1L".into()),
            is_special: false,
            is_club_price: false,
            in_stock: Some(true),
            availability: Some("IN_STOCK".into()),
            department: None,
            store_key: None,
            sponsored: false,
            image: None,
            url: String::new(),
        }
    }

    #[test]
    fn an_ordinary_price_is_just_the_price() {
        assert_eq!(price_label(&product(Some(719))), "$7.19");
        assert_eq!(price_label(&product(None)), "—");
    }

    #[test]
    fn a_special_shows_what_it_saves() {
        let p = Product {
            was_price_cents: Some(899),
            is_special: true,
            ..product(Some(719))
        };
        assert_eq!(price_label(&p), "$7.19 (was $8.99, save $1.80)");
    }

    #[test]
    fn a_special_with_no_was_price_still_says_it_is_one() {
        let p = Product {
            is_special: true,
            ..product(Some(719))
        };
        assert_eq!(price_label(&p), "$7.19 (special)");
    }

    #[test]
    fn a_member_price_is_badged_separately_from_a_special() {
        let p = Product {
            is_club_price: true,
            ..product(Some(719))
        };
        assert_eq!(price_label(&p), "$7.19 [member price]");
    }

    #[test]
    fn unit_pricing_needs_a_measure_to_read_well() {
        assert_eq!(
            unit_label(&product(Some(719))).as_deref(),
            Some("$2.40 per 1L")
        );
        let p = Product {
            unit_price_cents: None,
            ..product(Some(719))
        };
        assert_eq!(unit_label(&p), None);
    }
}
