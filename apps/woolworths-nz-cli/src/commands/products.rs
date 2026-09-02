//! `wwnz search`, `wwnz specials`, `wwnz browse` and `wwnz departments`.

use anyhow::{bail, Result};

use crate::api::SearchBy;
use crate::app::App;
use crate::cli::ListArgs;
use crate::commands::io::print_json;
use crate::domain::Product;
use crate::output;

/// Fetch a product list, apply the client-side `--size` filter, and render it.
pub async fn list(app: &App, by: SearchBy, specials_only: bool, args: &ListArgs) -> Result<()> {
    // Prices are per store and the store is a property of the session's cart,
    // so a selected store has to be bound before the search rather than passed
    // with it.
    let client = app.client().await?;
    let store = bind_store(app, &client).await?;

    let sort = args.sort.as_deref().unwrap_or_else(|| by.default_sort());
    let result = client.search(&by, args.limit, sort, specials_only).await?;
    let products = apply_size_filter(result.products, args.size.as_deref());

    if app.json {
        print_json(&serde_json::json!({
            "store_id": store,
            "specials_only": specials_only,
            "sort": sort,
            "count": products.len(),
            "total_available": result.total_available,
            "products": products,
        }));
        return Ok(());
    }

    if products.is_empty() {
        println!("{}", nothing_found(&by, args, specials_only));
        return Ok(());
    }

    let heading = store_label(app, store.as_deref());
    output::print_products(&products, heading.as_deref());
    if result.total_available as usize > products.len() {
        println!(
            "showing {} of {} matches; raise --limit for more.",
            products.len(),
            result.total_available
        );
    }
    Ok(())
}

/// `wwnz browse` -- resolve a department name to the key the API selects by.
pub async fn browse(
    app: &App,
    department: &str,
    specials_only: bool,
    args: &ListArgs,
) -> Result<()> {
    let client = app.client().await?;
    let root = client.categories().await?;
    let Some(category) = root.find(department) else {
        bail!("no department matches '{department}'. List them: wwnz departments");
    };
    // Say which one was chosen: "bakery" could reasonably mean more than one
    // thing, and silently picking is how the wrong list gets printed.
    if !app.json {
        println!("Browsing {} ({})\n", category.name, category.key);
    }
    list(
        app,
        SearchBy::Category(category.key.clone()),
        specials_only,
        args,
    )
    .await
}

/// `wwnz departments` -- the tree `browse` selects from.
pub async fn departments(app: &App, query: Option<&str>, depth: u32) -> Result<()> {
    let client = app.client().await?;
    let root = client.categories().await?;

    if app.json {
        print_json(&serde_json::json!({ "departments": root.children }));
        return Ok(());
    }
    output::print_categories(&root, query, depth);
    Ok(())
}

/// Bind the session's cart to the selected store, so prices come back for it.
///
/// A store is optional: with none selected the site prices against a default,
/// which is worth being able to do without setting one up first.
async fn bind_store(app: &App, client: &crate::api::Client) -> Result<Option<String>> {
    let Some(store) = app.config.store_id(app.store_flag.as_deref()) else {
        return Ok(None);
    };
    client.set_store(&store).await?;
    Ok(Some(store))
}

/// What to head the listing with.
///
/// The saved name only describes the saved id. When `--store` or the
/// environment named a different one, heading the results with the saved
/// store's name would put the wrong shop above the right prices, so the id
/// itself is shown instead.
fn store_label(app: &App, store: Option<&str>) -> Option<String> {
    let store = store?;
    if app.config.store_id.as_deref() == Some(store) {
        if let Some(name) = app.config.store_name.as_deref() {
            return Some(name.to_string());
        }
    }
    Some(store.to_string())
}

fn nothing_found(by: &SearchBy, args: &ListArgs, specials_only: bool) -> String {
    let mut what = match by {
        SearchBy::Keyword(q) => format!(" for '{q}'"),
        SearchBy::Category(_) => " in that department".to_string(),
        SearchBy::Specials | SearchBy::BuyAgain => String::new(),
    };
    if specials_only && !matches!(by, SearchBy::Specials) {
        what.push_str(" on special");
    }
    if let Some(size) = args.size.as_deref() {
        what.push_str(&format!(" matching '{size}'"));
    }
    format!("No products found{what}.")
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

    fn product(name: &str) -> Product {
        Product {
            sku: name.into(),
            variant_key: format!("{name}-EA"),
            name: name.into(),
            brand: None,
            unit_of_measure: None,
            price_cents: Some(100),
            was_price_cents: None,
            unit_price_cents: None,
            unit_measure: None,
            is_special: false,
            is_club_price: false,
            in_stock: Some(true),
            availability: None,
            department: None,
            store_key: None,
            sponsored: false,
            image: None,
            url: String::new(),
        }
    }

    #[test]
    fn size_filter_is_a_no_op_when_unset() {
        let products = vec![product("Milk Standard 2L")];
        assert_eq!(apply_size_filter(products.clone(), None).len(), 1);
        assert_eq!(apply_size_filter(products.clone(), Some("  ")).len(), 1);
        assert_eq!(apply_size_filter(products.clone(), Some("2L")).len(), 1);
        assert_eq!(apply_size_filter(products, Some("600ml")).len(), 0);
    }

    #[test]
    fn the_empty_message_names_whatever_narrowed_the_search() {
        let args = ListArgs {
            limit: 20,
            size: Some("2L".into()),
            sort: None,
        };
        let msg = nothing_found(&SearchBy::Keyword("milk".into()), &args, true);
        assert!(msg.contains("'milk'"), "{msg}");
        assert!(msg.contains("on special"), "{msg}");
        assert!(msg.contains("'2L'"), "{msg}");

        // `specials` is already about specials; saying so twice reads oddly.
        let bare = ListArgs {
            limit: 20,
            size: None,
            sort: None,
        };
        assert_eq!(
            nothing_found(&SearchBy::Specials, &bare, true),
            "No products found."
        );
    }
}
