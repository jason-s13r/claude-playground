//! Shops.

use std::io::{self, Write};

use bgnz_api::Store;
use cli_kit::{indented, table, Out, View};
use serde::Serialize;

#[derive(Serialize)]
pub struct StoreList<'a> {
    pub stores: &'a [Store],
    pub banner: String,
}

impl<'a> StoreList<'a> {
    pub fn new(stores: &'a [Store], banner: bgnz_api::Banner) -> StoreList<'a> {
        StoreList {
            stores,
            banner: banner.name().to_string(),
        }
    }
}

impl View for StoreList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.stores.is_empty() {
            return writeln!(out, "No {} stores found.", self.banner);
        }
        let mut t = table(&["ID", "Store", "Where", "Collect", "Cutoff"]);
        for s in self.stores {
            t.add_row(vec![
                // The id, not the fulfilment number: this is the one the stock
                // service takes, and the two are easy to confuse.
                s.id.to_string(),
                s.display_name.clone().unwrap_or_else(|| s.name.clone()),
                s.city.clone().unwrap_or_else(|| "—".into()),
                super::yes_no(s.click_and_collect),
                s.same_day_cutoff.clone().unwrap_or_else(|| "—".into()),
            ]);
        }
        writeln!(out, "{t}")?;
        super::write_count(
            out,
            self.stores.len(),
            "store",
            Some("Select one: `bgnz store set <id>`."),
        )
    }
}

/// The selected shop, for `store show`.
#[derive(Serialize)]
pub struct StoreView {
    pub banner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub hours: Vec<bgnz_api::OpeningHours>,
}

impl View for StoreView {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let Some(id) = self.store_id else {
            return writeln!(
                out,
                "No {} store set. Run `bgnz -b {} stores` and then `bgnz store set <id>`.",
                self.banner,
                self.banner.to_lowercase().replace(' ', "")
            );
        };
        writeln!(
            out,
            "{}",
            out.heading(self.name.as_deref().unwrap_or("Store"))
        )?;
        indented(out, "store id", &id.to_string())?;
        if let Some(address) = &self.address {
            indented(out, "address", address)?;
        }
        if !self.hours.is_empty() {
            let mut t = table(&["Day", "Open", "Close"]);
            for h in &self.hours {
                t.add_row(vec![
                    h.day.clone(),
                    h.open.clone().unwrap_or_else(|| "closed".into()),
                    h.close.clone().unwrap_or_else(|| "—".into()),
                ]);
            }
            writeln!(out, "{t}")?;
        }
        Ok(())
    }
}
