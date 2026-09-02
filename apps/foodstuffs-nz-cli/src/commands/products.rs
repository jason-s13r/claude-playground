//! `fsnz search`, `fsnz specials` and `fsnz browse` -- the commands that render
//! a product list.

use anyhow::Result;

use crate::api;
use crate::app::App;
use crate::banner::Banner;
use crate::cli::ListArgs;
use crate::commands::io::print_json;
use crate::domain::Product;
use crate::output;

/// Fetch a product list, apply the client-side `--size` filter, and render it.
pub async fn list(
    app: &App,
    banner: Banner,
    query: &str,
    department: Option<&str>,
    specials_only: bool,
    args: &ListArgs,
) -> Result<()> {
    let store_id = app.store_id(banner)?;
    // Product search prices against a store, not an account: it is guest-scoped
    // and an account token is rejected with "Invalid store id parameter in
    // filters". The cart is the other way round.
    let client = app.guest_client(banner).await?;
    let filters = api::filters(&store_id, specials_only, department);
    let result = client
        .collect(&store_id, query, &filters, args.limit, &args.sort)
        .await?;

    let products = apply_size_filter(result.products, args.size.as_deref());

    if app.json {
        print_json(&serde_json::json!({
            "banner": banner.id(),
            "store_id": store_id,
            "query": query,
            "department": department,
            "specials_only": specials_only,
            "count": products.len(),
            "total_available": result.total_available,
            "products": products,
        }));
        return Ok(());
    }

    if products.is_empty() {
        println!("{}", no_results_message(banner, query, department, args));
        return Ok(());
    }

    output::print_products(&products, banner);
    if result.total_available as usize > products.len() {
        println!(
            "showing {} of {} matches; raise --limit for more.",
            products.len(),
            result.total_available
        );
    }
    Ok(())
}

fn no_results_message(
    banner: Banner,
    query: &str,
    department: Option<&str>,
    args: &ListArgs,
) -> String {
    let mut what = if query.trim().is_empty() {
        String::new()
    } else {
        format!(" for '{query}'")
    };
    if let Some(dept) = department {
        what.push_str(&format!(" in '{dept}'"));
    }
    if let Some(size) = args.size.as_deref() {
        what.push_str(&format!(" with size matching '{size}'"));
    }
    format!("No products found{what} at {}.", banner.name())
}

pub(super) fn apply_size_filter(products: Vec<Product>, size: Option<&str>) -> Vec<Product> {
    match size.map(str::trim).filter(|s| !s.is_empty()) {
        Some(size) => products
            .into_iter()
            .filter(|p| p.matches_size(size))
            .collect(),
        None => products,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(name: &str, size: Option<&str>) -> Product {
        Product {
            sku: name.into(),
            banner: "newworld",
            name: name.into(),
            brand: None,
            size: size.map(str::to_string),
            price_cents: Some(100),
            unit_price_cents: None,
            unit_measure: None,
            multi_buy: None,
            is_special: false,
            in_stock: Some(true),
            department: None,
            image: None,
            url: String::new(),
        }
    }

    #[test]
    fn size_filter_is_a_no_op_when_unset() {
        let products = vec![product("Milk 2L", Some("2L"))];
        assert_eq!(apply_size_filter(products.clone(), None).len(), 1);
        assert_eq!(apply_size_filter(products.clone(), Some("  ")).len(), 1);
        assert_eq!(apply_size_filter(products.clone(), Some("2L")).len(), 1);
        assert_eq!(apply_size_filter(products, Some("600ml")).len(), 0);
    }
}
