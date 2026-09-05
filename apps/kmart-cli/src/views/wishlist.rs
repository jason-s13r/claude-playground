//! What is saved for later.

use std::io::{self, Write};

use cli_kit::{serde_json, table, Out, View};
use kmart_api::Wishlist;
use serde::Serialize;

#[derive(Serialize)]
pub struct WishlistView<'a> {
    pub wishlist: &'a Wishlist,
}

impl View for WishlistView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.wishlist.items.is_empty() {
            return writeln!(out, "Nothing saved.");
        }
        let mut t = table(&["Keycode", "Product", "Qty", "Price"]);
        for item in &self.wishlist.items {
            t.add_row(vec![
                item.keycode.clone(),
                super::or_dash(item.name.as_deref()),
                item.quantity.to_string(),
                item.price.map(super::money).unwrap_or_else(|| "—".into()),
            ]);
        }
        writeln!(out, "{t}")?;
        super::write_count(out, self.wishlist.items.len(), "item", None)?;
        // Said once, here, rather than left for a person to discover by
        // looking for a subcommand that is not there.
        writeln!(
            out,
            "{}",
            out.dim("Removing an item is not supported; see the kmart-api README.")
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
    use kmart_api::{Price, WishlistItem};

    fn render(wishlist: &Wishlist) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &WishlistView { wishlist }).unwrap();
        out.into_string()
    }

    #[test]
    fn a_list_shows_an_item_each() {
        let w = Wishlist {
            id: Some("l".into()),
            version: Some(2),
            items: vec![WishlistItem {
                id: "i1".into(),
                keycode: "43165537".into(),
                name: Some("Milk Frother".into()),
                quantity: 1,
                price: Some(Price::cents(3900)),
            }],
        };
        let text = render(&w);
        assert!(text.contains("43165537"), "{text}");
        assert!(text.contains("$39.00"), "{text}");
        assert!(text.contains("1 item."), "{text}");
    }

    #[test]
    fn the_missing_removal_is_stated_rather_than_left_to_be_discovered() {
        let w = Wishlist {
            id: None,
            version: None,
            items: vec![WishlistItem {
                id: "i1".into(),
                keycode: "1".into(),
                name: None,
                quantity: 1,
                price: None,
            }],
        };
        assert!(render(&w).contains("Removing an item is not supported"));
    }

    #[test]
    fn an_empty_list_says_so_without_the_caveat() {
        let w = Wishlist {
            id: None,
            version: None,
            items: Vec::new(),
        };
        assert_eq!(render(&w), "Nothing saved.\n");
    }
}
