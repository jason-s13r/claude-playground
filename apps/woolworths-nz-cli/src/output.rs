//! Human-readable rendering. `--json` bypasses all of this.

mod cart;
mod categories;
mod orders;
mod products;
mod stores;

pub use cart::print_cart;
pub use categories::print_categories;
pub use orders::print_orders;
pub use products::print_products;
pub use stores::print_stores;

use comfy_table::{presets, ContentArrangement, Table};

fn table() -> Table {
    let mut t = Table::new();
    t.load_preset(presets::UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic);
    t
}

pub fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Name the store something is priced against. Woolworths store names already
/// carry the brand ("Regent Woolworths"), so prefixing it again just stutters.
pub(crate) fn store_heading(store: Option<&str>) -> String {
    match store {
        Some(name) if name.to_lowercase().contains("woolworths") => name.to_string(),
        Some(name) => format!("Woolworths — {name}"),
        None => "Woolworths".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals() {
        assert_eq!(plural(1), "");
        assert_eq!(plural(0), "s");
        assert_eq!(plural(2), "s");
    }

    #[test]
    fn a_store_name_that_already_says_the_brand_does_not_repeat_it() {
        assert_eq!(
            store_heading(Some("Regent Woolworths")),
            "Regent Woolworths"
        );
        assert_eq!(store_heading(Some("9048")), "Woolworths — 9048");
        assert_eq!(store_heading(None), "Woolworths");
    }
}
