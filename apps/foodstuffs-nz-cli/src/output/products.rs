//! Rendering a product listing.

use owo_colors::{OwoColorize, Stream};

use crate::banner::Banner;
use crate::domain::{dollars, Product};
use crate::output::plural;

pub(super) fn price_label(p: &Product) -> String {
    match p.price_cents {
        Some(c) if p.is_special => format!("{} (special)", dollars(c)),
        Some(c) => dollars(c),
        None => "—".to_string(),
    }
}

fn unit_label(p: &Product) -> Option<String> {
    match (p.unit_price_cents, p.unit_measure.as_deref()) {
        (Some(c), Some(m)) => Some(format!("{} per {m}", dollars(c))),
        (Some(c), None) => Some(dollars(c)),
        _ => None,
    }
}

/// Grouped by brand and name, so the size variants of one product sit together
/// rather than scattered through the results.
pub fn print_products(products: &[Product], banner: Banner) {
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
        banner.name(),
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
            let mut head = Vec::new();
            if let Some(size) = p.size.as_deref().filter(|s| !s.is_empty()) {
                head.push(format!("Size: {size}"));
            }
            head.push(format!("Price: {}", price_label(p)));
            if let Some(mb) = &p.multi_buy {
                head.push(format!("Multi-buy: {mb}"));
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
            if let Some(unit) = unit_label(p) {
                tail.push(unit);
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

    fn product(cents: Option<i64>, special: bool) -> Product {
        Product {
            sku: "A-EA-000".into(),
            banner: "newworld",
            name: "Blue Milk".into(),
            brand: Some("Anchor".into()),
            size: Some("2L".into()),
            price_cents: cents,
            unit_price_cents: Some(225),
            unit_measure: Some("1L".into()),
            multi_buy: None,
            is_special: special,
            in_stock: Some(true),
            department: None,
            image: None,
            url: "https://example.test/p".into(),
        }
    }

    #[test]
    fn prices_say_when_something_is_on_special() {
        assert_eq!(price_label(&product(Some(450), false)), "$4.50");
        assert_eq!(price_label(&product(Some(399), true)), "$3.99 (special)");
        assert_eq!(price_label(&product(None, false)), "—");
    }

    #[test]
    fn unit_pricing_needs_a_measure_to_be_useful() {
        assert_eq!(
            unit_label(&product(Some(450), false)).as_deref(),
            Some("$2.25 per 1L")
        );
    }
}
