//! A store list.

use std::io::{self, Write};

use cli_kit::{table, Out, View};
use gsnz_core::Store;
use serde::Serialize;

#[derive(Serialize)]
#[serde(transparent)]
pub struct StoreList<'a> {
    pub stores: &'a [Store],
    #[serde(skip)]
    pub next: Option<&'a str>,
}

impl<'a> StoreList<'a> {
    pub fn new(stores: &'a [Store]) -> StoreList<'a> {
        StoreList { stores, next: None }
    }

    /// What to do with the list, supplied by the caller.
    ///
    /// The text names a command, and this crate does not know what the command is
    /// called: the same listing is `gsnz store set` here and `fsnz store set` in
    /// the tool this was lifted from. Left off, the footer is the count alone.
    pub fn next(mut self, next: &'a str) -> StoreList<'a> {
        self.next = Some(next);
        self
    }
}

impl View for StoreList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.stores.is_empty() {
            return writeln!(out, "No stores matched.");
        }
        // Distance is only known for a location search, and an empty column
        // across every row is worse than no column.
        let has_distance = self.stores.iter().any(|s| s.distance_km.is_some());
        let headers: &[&str] = if has_distance {
            &["ID", "Name", "Where", "Distance"]
        } else {
            &["ID", "Name", "Where"]
        };

        let mut t = table(headers);
        for s in self.stores {
            let mut row = vec![
                s.id.clone(),
                s.name.clone(),
                s.where_it_is().unwrap_or_default(),
            ];
            if has_distance {
                row.push(
                    s.distance_km
                        .map(|km| format!("{km:.1} km"))
                        .unwrap_or_default(),
                );
            }
            t.add_row(row);
        }
        writeln!(out, "{t}")?;
        crate::write_count(out, self.stores.len(), "store", self.next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use gsnz_core::RetailerId;

    fn store(id: &str, name: &str, distance: Option<f64>) -> Store {
        Store {
            retailer: RetailerId::Woolworths,
            id: id.into(),
            name: name.into(),
            address: None,
            area: Some("Regent".into()),
            city: Some("Whangarei".into()),
            distance_km: distance,
        }
    }

    fn render(stores: &[Store]) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(
            &mut out,
            &StoreList::new(stores).next("my-tool store set <id>"),
        )
        .unwrap();
        out.into_string()
    }

    #[test]
    fn the_distance_column_appears_only_when_something_has_one() {
        let text = render(&[store("1", "Regent", None)]);
        assert!(!text.contains("Distance"), "{text}");

        let text = render(&[store("1", "Regent", Some(2.34))]);
        assert!(text.contains("Distance"), "{text}");
        assert!(text.contains("2.3 km"), "{text}");
    }

    #[test]
    fn no_matches_says_so_rather_than_printing_an_empty_table() {
        assert_eq!(render(&[]), "No stores matched.\n");
    }

    #[test]
    fn the_count_is_pluralised_and_names_the_next_command() {
        let text = render(&[store("1", "Regent", None)]);
        assert!(text.contains("1 store."), "{text}");
        // Supplied by the caller: this crate does not know the command's name.
        assert!(text.contains("my-tool store set"), "{text}");

        let text = render(&[store("1", "A", None), store("2", "B", None)]);
        assert!(text.contains("2 stores."), "{text}");
    }
}
