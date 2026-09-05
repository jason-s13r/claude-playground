//! Where something can actually be had.
//!
//! A channel each rather than a yes or no. An item can be sold out for
//! delivery and sitting on a shelf a suburb away, and a single "in stock"
//! column would have to pick one of those to lie about.

use std::io::{self, Write};

use cli_kit::{serde_json, table, Out, View};
use kmart_api::Availability;
use serde::Serialize;

#[derive(Serialize)]
pub struct StockView<'a> {
    pub availability: &'a Availability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<&'a str>,
}

impl View for StockView<'_> {
    fn text(&self, out: &mut Out) -> io::Result<()> {
        if let Some(name) = self.product {
            writeln!(out, "{}", out.heading(name))?;
        }
        write_summary(out, self.availability)
    }

    fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// The channel table and the per-store rows. Shared with the product view, so
/// `kmart product` and `kmart stock` cannot drift apart.
pub(crate) fn write_summary(out: &mut Out, a: &Availability) -> io::Result<()> {
    let mut t = table(&["Channel", "Available"]);
    for (label, count) in [
        ("Home delivery", a.home_delivery),
        ("Express delivery", a.express),
        ("Click & collect", a.click_and_collect),
    ] {
        if let Some(count) = count {
            t.add_row(vec![label.to_string(), count.to_string()]);
        }
    }
    writeln!(out, "{t}")?;

    // Naming a store is a request each, so the product view does not pay for
    // it. Bare ids in a table are not an answer to "where can I get this", so
    // when none of them has a name this points at the command that does.
    let named = a.stores.iter().any(|s| s.name.is_some());
    if !a.stores.is_empty() && !named {
        writeln!(
            out,
            "{}",
            out.dim(&format!(
                "In {} nearby stores; `kmart stock {}` names them.",
                a.stores.iter().filter(|s| s.available > 0).count(),
                a.keycode
            ))
        )?;
    } else if !a.stores.is_empty() {
        let mut s = table(&["Store", "Available", "Distance"]);
        for store in &a.stores {
            s.add_row(vec![
                match &store.name {
                    Some(name) => format!("{name} ({})", store.store_id),
                    None => store.store_id.clone(),
                },
                match store.buddy {
                    // Its stock fills orders for another store, so it is not
                    // necessarily on a shelf you can walk to.
                    true => format!("{} (buddy store)", store.available),
                    false => store.available.to_string(),
                },
                store
                    .distance_km
                    .map(|km| format!("{km:.1} km"))
                    .unwrap_or_else(|| "—".into()),
            ]);
        }
        writeln!(out, "{s}")?;
    }

    if !a.any() {
        writeln!(out, "Not available anywhere near {}.", a.postcode)?;
    } else {
        writeln!(out, "{}", out.dim(&format!("Near {}.", a.postcode)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::{emit, Format};
    use kmart_api::StoreStock;

    fn availability() -> Availability {
        Availability {
            keycode: "43165537".into(),
            postcode: "1010".into(),
            region: Some("METRO".into()),
            home_delivery: Some(61),
            pool: None,
            express: None,
            click_and_collect: Some(93),
            stores: vec![StoreStock {
                store_id: "8229".into(),
                name: Some("Sylvia Park".into()),
                available: 26,
                distance_km: Some(16.6),
                buddy: false,
            }],
            in_store: Vec::new(),
        }
    }

    fn render(a: &Availability) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(
            &mut out,
            &StockView {
                availability: a,
                product: None,
            },
        )
        .unwrap();
        out.into_string()
    }

    #[test]
    fn each_channel_is_reported_separately() {
        let text = render(&availability());
        assert!(text.contains("Home delivery"), "{text}");
        assert!(text.contains("61"), "{text}");
        assert!(text.contains("Click & collect"), "{text}");
        assert!(text.contains("Sylvia Park (8229)"), "{text}");
        assert!(text.contains("16.6 km"), "{text}");
    }

    #[test]
    fn a_channel_the_gateway_said_nothing_about_is_left_out() {
        // Express came back null, which is not the same as zero.
        let text = render(&availability());
        assert!(!text.contains("Express"), "{text}");
    }

    #[test]
    fn sold_out_online_and_on_a_shelf_are_both_shown() {
        let mut a = availability();
        a.home_delivery = Some(0);
        let text = render(&a);
        assert!(text.contains("Home delivery"), "{text}");
        assert!(!text.contains("Not available"), "the shelf still counts");
    }

    #[test]
    fn nothing_anywhere_says_so_plainly() {
        let mut a = availability();
        a.home_delivery = Some(0);
        a.click_and_collect = Some(0);
        a.stores[0].available = 0;
        let text = render(&a);
        assert!(text.contains("Not available anywhere near 1010"), "{text}");
    }

    #[test]
    fn unnamed_stores_point_at_the_command_that_names_them() {
        // The product view skips the per-store lookup, so a table of bare ids
        // would be worse than a sentence.
        let mut a = availability();
        a.stores[0].name = None;
        let text = render(&a);
        assert!(text.contains("kmart stock 43165537"), "{text}");
        assert!(!text.contains("8229"), "no table of bare ids: {text}");
    }

    #[test]
    fn a_buddy_store_is_marked_because_its_stock_is_not_on_that_shelf() {
        let mut a = availability();
        a.stores[0].buddy = true;
        assert!(render(&a).contains("buddy store"));
    }
}
