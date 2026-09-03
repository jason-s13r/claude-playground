//! A store list.

use std::io::{self, Write};

use cli_kit::{plural, table, Out, View};
use gsnz_core::Store;
use serde::Serialize;

#[derive(Serialize)]
#[serde(transparent)]
pub struct StoreList<'a>(pub &'a [Store]);

impl View for StoreList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.0.is_empty() {
            return writeln!(out, "No stores matched.");
        }
        // Distance is only known for a location search, and an empty column
        // across every row is worse than no column.
        let has_distance = self.0.iter().any(|s| s.distance_km.is_some());
        let headers: &[&str] = if has_distance {
            &["ID", "Name", "Where", "Distance"]
        } else {
            &["ID", "Name", "Where"]
        };

        let mut t = table(headers);
        for s in self.0 {
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
        writeln!(
            out,
            "{} store{}. Select one: gsnz store set <id or name fragment>",
            self.0.len(),
            plural(self.0.len())
        )
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
        emit(&mut out, &StoreList(stores)).unwrap();
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
        assert!(text.contains("gsnz store set"), "{text}");

        let text = render(&[store("1", "A", None), store("2", "B", None)]);
        assert!(text.contains("2 stores."), "{text}");
    }
}
