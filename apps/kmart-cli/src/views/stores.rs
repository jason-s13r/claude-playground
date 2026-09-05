//! Stores, near a postcode or one at a time.

use std::io::{self, Write};

use cli_kit::{serde_json, table, Out, View};
use kmart_api::Store;
use serde::Serialize;

#[derive(Serialize)]
pub struct StoreList<'a> {
    pub stores: &'a [Store],
    pub postcode: &'a str,
}

impl View for StoreList<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if self.stores.is_empty() {
            return writeln!(out, "No stores near {}.", self.postcode);
        }
        let mut t = table(&["ID", "Store", "Suburb", "Distance"]);
        for s in self.stores {
            t.add_row(vec![
                s.id.clone(),
                s.name.clone(),
                super::or_dash(s.city.as_deref()),
                s.distance_km
                    .map(|km| format!("{km:.1} km"))
                    .unwrap_or_else(|| "—".into()),
            ]);
        }
        writeln!(out, "{t}")?;
        super::write_count(
            out,
            self.stores.len(),
            "store",
            Some(&format!("Near {}.", self.postcode)),
        )
    }

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Serialize)]
pub struct StoreView<'a> {
    pub store: &'a Store,
}

impl View for StoreView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        let s = self.store;
        writeln!(out, "{}", out.heading(&s.name))?;
        let mut t = table(&["", ""]);
        t.add_row(vec!["ID".to_string(), s.id.clone()]);
        if !s.address.is_empty() {
            t.add_row(vec!["Address".to_string(), s.address.join(", ")]);
        }
        for (label, value) in [
            ("Suburb", s.city.as_deref()),
            ("State", s.state.as_deref()),
            ("Postcode", s.postcode.as_deref()),
            ("Phone", s.phone.as_deref()),
        ] {
            if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
                t.add_row(vec![label.to_string(), value.to_string()]);
            }
        }
        if let Some(km) = s.distance_km {
            t.add_row(vec!["Distance".to_string(), format!("{km:.1} km")]);
        }
        writeln!(out, "{t}")?;

        if !s.hours.is_empty() {
            let mut h = table(&["Day", "Hours"]);
            for day in &s.hours {
                h.add_row(vec![day.day.clone(), day.hours.clone()]);
            }
            writeln!(out, "{h}")?;
        }
        Ok(())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use kmart_api::TradingDay;

    fn store() -> Store {
        Store {
            id: "8229".into(),
            name: "Sylvia Park".into(),
            phone: Some("+64 9 573 9200".into()),
            address: vec!["286 Mt Wellington Highway".into()],
            city: Some("Auckland".into()),
            state: Some("North Island".into()),
            postcode: Some("1060".into()),
            latitude: None,
            longitude: None,
            distance_km: Some(16.6),
            hours: vec![TradingDay {
                day: "Monday".into(),
                hours: "9:00am - 7:00pm".into(),
            }],
        }
    }

    fn render<V: View>(view: &V) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, view).unwrap();
        out.into_string()
    }

    #[test]
    fn a_store_listing_is_ordered_as_the_gateway_returned_it() {
        let stores = vec![store()];
        let text = render(&StoreList {
            stores: &stores,
            postcode: "1010",
        });
        assert!(text.contains("Sylvia Park"), "{text}");
        assert!(text.contains("16.6 km"), "{text}");
        assert!(text.contains("1 store."), "{text}");
    }

    #[test]
    fn no_stores_says_where_it_looked() {
        let text = render(&StoreList {
            stores: &[],
            postcode: "9999",
        });
        assert!(text.contains("No stores near 9999"), "{text}");
    }

    #[test]
    fn one_store_shows_its_address_and_hours() {
        let s = store();
        let text = render(&StoreView { store: &s });
        assert!(text.contains("286 Mt Wellington Highway"), "{text}");
        assert!(text.contains("+64 9 573 9200"), "{text}");
        assert!(text.contains("Monday"), "{text}");
    }

    #[test]
    fn a_blank_field_is_left_out_rather_than_printed_empty() {
        let mut s = store();
        s.phone = None;
        s.state = Some("   ".into());
        let text = render(&StoreView { store: &s });
        assert!(!text.contains("Phone"), "{text}");
        assert!(!text.contains("State"), "{text}");
    }
}
