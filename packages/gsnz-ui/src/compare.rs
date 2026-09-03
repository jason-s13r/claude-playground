//! Retailers side by side.
//!
//! The one thing this view must never do is present a guess as a fact. A
//! comparison that silently equates two different two-litre milks is a
//! wrong-price bug, so a row matched by description rather than by product code
//! is marked, and the marker is explained under the table.

use std::io::{self, Write};

use cli_kit::comfy_table::Cell;
use cli_kit::{plural, table, Out, View};
use gsnz_core::compare::Match;
use gsnz_core::{dollars, RetailerId, Row};
use serde::Serialize;

use crate::products::price_label;

#[derive(Serialize)]
pub struct CompareTable<'a> {
    pub retailers: &'a [RetailerId],
    pub rows: &'a [Row],
}

impl View for CompareTable<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.rows.is_empty() {
            return writeln!(out, "Nothing found at any of them.");
        }

        let mut headers: Vec<&str> = vec!["Product", "Size"];
        headers.extend(self.retailers.iter().map(|r| r.name()));
        headers.push("Difference");
        let mut t = table(&headers);

        for row in self.rows {
            let cheapest = row.cheapest();
            let mut cells: Vec<Cell> = vec![
                Cell::new(&row.title),
                Cell::new(row.size.clone().unwrap_or_default()),
            ];
            for (i, side) in row.sides.iter().enumerate() {
                let text = match side {
                    Some(p) => {
                        let mut label = price_label(p);
                        // Only the columns that were attached by description
                        // carry the marker; the row's own catalogue column is
                        // exact by construction.
                        if row.match_kind == Match::Normalised
                            && p.retailer.catalogue() != row_catalogue(row)
                        {
                            label = format!("~ {label}");
                        }
                        if cheapest == Some(i) {
                            label.push_str("  ←");
                        }
                        label
                    }
                    // Absence means "not in this retailer's results", which is
                    // not "unavailable" -- each is searched independently and
                    // the limit may simply have cut it off.
                    None => "—".to_string(),
                };
                cells.push(Cell::new(text));
            }
            cells.push(Cell::new(match row.saving() {
                Some(0) => "same".to_string(),
                Some(c) => dollars(c),
                None => String::new(),
            }));
            t.add_row(cells);
        }
        writeln!(out, "{t}")?;

        let matched = self.rows.iter().filter(|r| r.matched()).count();
        let fuzzy = self
            .rows
            .iter()
            .filter(|r| r.match_kind == Match::Normalised && r.matched())
            .count();
        writeln!(
            out,
            "{} product{} compared, {} found at more than one.",
            self.rows.len(),
            plural(self.rows.len()),
            matched,
        )?;
        writeln!(
            out,
            "←  cheapest.   —  not in that retailer's results, which is not the same as unavailable."
        )?;
        if fuzzy > 0 {
            writeln!(
                out,
                "{}",
                out.warn(&format!(
                    "~  matched by name and size, not by product code ({fuzzy} row{}). Check before trusting the difference.",
                    plural(fuzzy)
                ))
            )?;
        }
        Ok(())
    }
}

/// The catalogue that named a row, which is the side that is exact.
fn row_catalogue(row: &Row) -> Option<&'static str> {
    row.sides
        .iter()
        .flatten()
        .find_map(|p| p.retailer.catalogue())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use gsnz_core::{pair, Product, SaleUnit};

    fn product(retailer: RetailerId, sku: &str, name: &str, size: &str, cents: i64) -> Product {
        Product {
            retailer,
            sku: sku.into(),
            key: sku.into(),
            name: name.into(),
            brand: None,
            size: Some(size.into()),
            price_cents: Some(cents),
            was_price_cents: None,
            unit_price_cents: None,
            unit_measure: None,
            sale_unit: SaleUnit::Each,
            multi_buy: None,
            is_special: false,
            is_member_price: false,
            in_stock: Some(true),
            availability: None,
            department: None,
            image: None,
            url: None,
        }
    }

    fn render(retailers: &[RetailerId], rows: &[Row]) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, &CompareTable { retailers, rows }).unwrap();
        out.into_string()
    }

    #[test]
    fn an_exact_pairing_carries_no_marker_and_no_footnote() {
        let rows = pair(
            &[
                vec![product(RetailerId::NewWorld, "A", "Milk", "2L", 450)],
                vec![product(RetailerId::PaknSave, "A", "Milk", "2L", 399)],
            ],
            true,
        );
        let text = render(&[RetailerId::NewWorld, RetailerId::PaknSave], &rows);
        assert!(!text.contains('~'), "no fuzzy marker: {text}");
        assert!(text.contains("←"), "the cheaper side is marked: {text}");
        assert!(text.contains("$0.51"), "{text}");
    }

    #[test]
    fn a_fuzzy_column_is_marked_and_explained() {
        let rows = pair(
            &[
                vec![product(
                    RetailerId::NewWorld,
                    "A",
                    "Anchor Blue Milk",
                    "2L",
                    450,
                )],
                vec![product(
                    RetailerId::Woolworths,
                    "282768",
                    "Anchor Blue Milk",
                    "2 litre",
                    520,
                )],
            ],
            true,
        );
        let text = render(&[RetailerId::NewWorld, RetailerId::Woolworths], &rows);
        assert!(text.contains("~ "), "the fuzzy column is marked: {text}");
        assert!(
            text.contains("matched by name and size, not by product code"),
            "and explained: {text}"
        );
        assert!(text.contains("1 row)"), "{text}");
    }

    #[test]
    fn a_missing_side_is_not_called_unavailable() {
        let rows = pair(
            &[
                vec![product(RetailerId::NewWorld, "A", "Only here", "2L", 450)],
                vec![],
            ],
            false,
        );
        let text = render(&[RetailerId::NewWorld, RetailerId::PaknSave], &rows);
        assert!(text.contains("—"), "{text}");
        assert!(text.contains("not the same as unavailable"), "{text}");
    }

    #[test]
    fn equal_prices_read_as_same_rather_than_zero() {
        let rows = pair(
            &[
                vec![product(RetailerId::NewWorld, "A", "Milk", "2L", 400)],
                vec![product(RetailerId::PaknSave, "A", "Milk", "2L", 400)],
            ],
            false,
        );
        let text = render(&[RetailerId::NewWorld, RetailerId::PaknSave], &rows);
        assert!(text.contains("same"), "{text}");
        // The legend always mentions the marker, so look at the row itself.
        let row = text.lines().find(|l| l.contains("Milk")).unwrap();
        assert!(!row.contains('←'), "no winner when they are equal: {row}");
    }

    #[test]
    fn nothing_anywhere_says_so() {
        assert_eq!(
            render(&[RetailerId::NewWorld], &[]),
            "Nothing found at any of them.\n"
        );
    }

    #[test]
    fn json_records_the_match_quality_per_row() {
        let rows = pair(
            &[
                vec![product(
                    RetailerId::NewWorld,
                    "A",
                    "Anchor Blue Milk",
                    "2L",
                    450,
                )],
                vec![product(
                    RetailerId::Woolworths,
                    "B",
                    "Anchor Blue Milk",
                    "2L",
                    520,
                )],
            ],
            true,
        );
        let mut out = Out::buffer(Format::Json);
        emit(
            &mut out,
            &CompareTable {
                retailers: &[RetailerId::NewWorld, RetailerId::Woolworths],
                rows: &rows,
            },
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&out.into_string()).unwrap();
        assert_eq!(value["rows"][0]["match"], "normalised");
    }
}
