//! Which stores have a thing.
//!
//! The one listing whose data is genuinely uncertain: the endpoint answers with
//! rendered markup, so `in_stock` is what could be read out of a CSS class and
//! `label` is the words themselves. When the two disagree the words win, on the
//! grounds that they are what the site chose to say.

use std::io::{self, Write};

use cli_kit::{table, Out, View};
use serde::Serialize;
use twlnz_api::StoreStock;

#[derive(Serialize)]
pub struct StockList<'a> {
    pub pid: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<&'a str>,
    pub stores: &'a [StoreStock],
}

impl View for StockList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if let Some(name) = self.product {
            writeln!(out, "{}", out.heading(name))?;
        }
        if self.stores.is_empty() {
            return writeln!(
                out,
                "No stores reported stock for {}{}.",
                self.pid,
                match self.region {
                    Some(region) => format!(" in {region}"),
                    None => String::new(),
                }
            );
        }

        let mut t = table(&["Store", "Stock"]);
        for s in self.stores {
            t.add_row(vec![
                s.store_name.clone(),
                // The label is the site's own wording, so it is preferred over
                // any word invented here. Plain: a coloured cell is measured by
                // its bytes and breaks the column rules.
                match (s.in_stock, &s.label) {
                    (_, Some(label)) => label.clone(),
                    (Some(true), None) => "in stock".into(),
                    (Some(false), None) => "not available".into(),
                    (None, None) => "—".into(),
                },
            ]);
        }
        writeln!(out, "{t}")?;

        let with = self
            .stores
            .iter()
            .filter(|s| s.in_stock == Some(true))
            .count();
        super::write_count(
            out,
            self.stores.len(),
            "store",
            Some(&format!("{with} with stock.")),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};

    fn stock(name: &str, in_stock: Option<bool>, label: &str) -> StoreStock {
        StoreStock {
            store_id: None,
            store_name: name.into(),
            label: Some(label.into()),
            in_stock,
        }
    }

    fn render(list: &StockList<'_>) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, list).unwrap();
        out.into_string()
    }

    #[test]
    fn stock_is_counted_as_well_as_listed() {
        let stores = vec![
            stock("Example Town", Some(true), "In stock"),
            stock("Other Town", Some(false), "Not available"),
        ];
        let text = render(&StockList {
            pid: "R1",
            product: Some("A Thing"),
            region: None,
            stores: &stores,
        });
        assert!(text.contains("A Thing"), "{text}");
        assert!(text.contains("2 stores. 1 with stock."), "{text}");
    }

    #[test]
    fn the_sites_own_wording_is_preferred_over_a_word_invented_here() {
        let stores = vec![stock("Example Town", Some(true), "Low stock")];
        let text = render(&StockList {
            pid: "R1",
            product: None,
            region: None,
            stores: &stores,
        });
        assert!(text.contains("Low stock"), "{text}");
        assert!(!text.contains("in stock"), "{text}");
    }

    #[test]
    fn no_stock_anywhere_says_so_including_the_region_asked_about() {
        let text = render(&StockList {
            pid: "R1",
            product: None,
            region: Some("NZ-CAN"),
            stores: &[],
        });
        assert!(
            text.contains("No stores reported stock for R1 in NZ-CAN"),
            "{text}"
        );
    }
}
