//! What is saved for later.

use std::io::{self, Write};

use cli_kit::{table, Out, View};
use serde::Serialize;
use twlnz_api::Wishlist;

#[derive(Serialize)]
pub struct WishlistView<'a> {
    #[serde(flatten)]
    pub wishlist: &'a Wishlist,
    /// What just happened, when this is shown after a change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl<'a> WishlistView<'a> {
    pub fn new(wishlist: &'a Wishlist) -> WishlistView<'a> {
        WishlistView {
            wishlist,
            note: None,
        }
    }

    pub fn after(mut self, note: impl Into<String>) -> WishlistView<'a> {
        self.note = Some(note.into());
        self
    }
}

impl View for WishlistView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if let Some(note) = &self.note {
            writeln!(out, "{note}")?;
        }
        if self.wishlist.items.is_empty() {
            return writeln!(out, "Nothing is saved.");
        }

        // One price column, not the cart's two: saving something quotes no line
        // total, and multiplying the unit price here would invent one.
        let mut t = table(&["ID", "Product", "Qty", "Price", "Stock"]);
        for item in &self.wishlist.items {
            let name = match item.variation.is_empty() {
                true => item.name.clone(),
                // The variation is part of what was saved, and without it two
                // colours of one garment are the same row twice.
                false => format!("{} ({})", item.name, item.variation.join(", ")),
            };
            t.add_row(vec![
                item.id.clone(),
                name,
                item.quantity.to_string(),
                item.price.label().unwrap_or_else(|| "—".into()),
                item.stock.clone().unwrap_or_else(|| "—".into()),
            ]);
        }
        writeln!(out, "{t}")?;

        // Rows, not units: a wishlist counts things saved, and its own heading
        // is what tells this apart from what fitted on one page.
        let shown = self.wishlist.items.len();
        match self.wishlist.total {
            Some(total) if !self.wishlist.complete() => {
                writeln!(
                    out,
                    "{shown} of {total} saved; the rest are on later pages."
                )
            }
            // Not the shared `write_count`: it pluralises the noun it is given,
            // and "saved" is not one.
            _ => writeln!(out, "{shown} saved."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use twlnz_api::{Price, WishlistItem};

    fn item() -> WishlistItem {
        WishlistItem {
            uuid: "row-1".into(),
            id: "R1".into(),
            name: "A Thing".into(),
            quantity: 2,
            price: Price::from_display("$12.00"),
            variation: vec!["Green Dark".into(), "S".into()],
            stock: Some("In stock".into()),
            url: None,
            image: None,
            add_to_cart: None,
        }
    }

    fn render(view: &WishlistView<'_>) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, view).unwrap();
        out.into_string()
    }

    #[test]
    fn a_saved_variant_is_named_by_what_was_chosen() {
        let wishlist = Wishlist {
            items: vec![item(), item()],
            total: Some(2),
        };
        let text = render(&WishlistView::new(&wishlist));
        assert!(text.contains("A Thing (Green Dark, S)"), "{text}");
        assert!(text.contains("In stock"), "{text}");
        assert!(text.contains("2 saved."), "{text}");
    }

    #[test]
    fn a_list_longer_than_its_page_says_so_rather_than_undercounting() {
        let wishlist = Wishlist {
            items: vec![item()],
            total: Some(34),
        };
        let text = render(&WishlistView::new(&wishlist));
        assert!(text.contains("1 of 34 saved"), "{text}");
    }

    #[test]
    fn an_empty_wishlist_says_so() {
        let wishlist = Wishlist::default();
        assert_eq!(render(&WishlistView::new(&wishlist)), "Nothing is saved.\n");
    }
}
