//! Human-readable rendering. `--json` bypasses all of this.

mod cart;
mod compare;
mod orders;
mod products;
mod stores;

pub use cart::print_cart;
pub use compare::print_comparison;
pub use orders::{print_order, print_orders, print_previous};
pub use products::print_products;
pub use stores::print_stores;

use comfy_table::{presets, ContentArrangement, Table};

use crate::banner::Banner;

/// Name the place something belongs to. Store names usually already carry the
/// banner ("New World Thorndon"), so prefixing it again just stutters.
pub(crate) fn store_heading(store: Option<&str>, banner: Banner) -> String {
    match store {
        Some(name) if name.to_lowercase().contains(&banner.name().to_lowercase()) => {
            name.to_string()
        }
        Some(name) => format!("{} — {name}", banner.name()),
        None => banner.name().to_string(),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_name_that_already_says_the_banner_does_not_repeat_it() {
        assert_eq!(
            store_heading(Some("New World Thorndon"), Banner::NewWorld),
            "New World Thorndon"
        );
        assert_eq!(
            store_heading(Some("4147"), Banner::NewWorld),
            "New World — 4147"
        );
        assert_eq!(store_heading(None, Banner::PaknSave), "PAK'nSAVE");
    }

    #[test]
    fn plurals() {
        assert_eq!(plural(1), "");
        assert_eq!(plural(0), "s");
        assert_eq!(plural(2), "s");
    }
}
