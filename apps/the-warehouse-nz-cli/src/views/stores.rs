//! Shops.

use std::io::{self, Write};

use cli_kit::{table, Out, View};
use serde::Serialize;
use twlnz_api::Store;

#[derive(Serialize)]
pub struct StoreList<'a> {
    pub stores: &'a [Store],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<&'a str>,
}

impl<'a> StoreList<'a> {
    pub fn new(stores: &'a [Store], region: Option<&'a str>) -> StoreList<'a> {
        StoreList { stores, region }
    }
}

impl View for StoreList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.stores.is_empty() {
            return writeln!(out, "No stores found.");
        }
        let mut t = table(&["ID", "Store", "Where", "Open", "Click & collect"]);
        for s in self.stores {
            t.add_row(vec![
                s.id.clone(),
                s.name.clone(),
                s.city.clone().unwrap_or_else(|| "—".into()),
                // Plain: a coloured cell is measured by its bytes and breaks
                // the column rules. See `views::stock_label`.
                match (s.open_now, &s.hours_today) {
                    (Some(true), Some(hours)) => format!("now, {hours}"),
                    (Some(false), Some(hours)) => format!("closed, {hours}"),
                    (Some(true), None) => "now".into(),
                    (Some(false), None) => "closed".into(),
                    (None, Some(hours)) => hours.clone(),
                    (None, None) => "—".into(),
                },
                match s.click_and_collect {
                    Some(true) => "yes".into(),
                    Some(false) => "no".into(),
                    None => "—".into(),
                },
            ]);
        }
        writeln!(out, "{t}")?;
        super::write_count(
            out,
            self.stores.len(),
            "store",
            Some("Select one: `twlnz store set <id>`."),
        )
    }
}

/// The selected store, for `store show`.
#[derive(Serialize)]
pub struct StoreView {
    pub store_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl View for StoreView {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        match (&self.store_id, &self.name) {
            (Some(id), Some(name)) => writeln!(out, "{name} ({id})"),
            (Some(id), None) => writeln!(out, "{id}"),
            (None, _) => writeln!(
                out,
                "No store selected. Run `twlnz store set <id>`; `twlnz stores` lists them."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};

    fn render<V: View>(view: &V) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, view).unwrap();
        out.into_string()
    }

    #[test]
    fn a_store_list_says_how_to_choose_one() {
        let stores = vec![Store {
            id: "119".into(),
            name: "Example Town".into(),
            city: Some("Auckland".into()),
            open_now: Some(true),
            hours_today: Some("8.00am - 9.00pm".into()),
            click_and_collect: Some(true),
            ..Store::default()
        }];
        let text = render(&StoreList::new(&stores, Some("NZ-AUK")));
        assert!(text.contains("Example Town"), "{text}");
        assert!(text.contains("1 store. Select one"), "{text}");
    }

    #[test]
    fn an_empty_region_says_so_rather_than_printing_an_empty_table() {
        assert_eq!(render(&StoreList::new(&[], None)), "No stores found.\n");
    }

    #[test]
    fn no_selected_store_says_what_to_run() {
        let text = render(&StoreView {
            store_id: None,
            name: None,
        });
        assert!(text.contains("twlnz store set"), "{text}");
    }
}
