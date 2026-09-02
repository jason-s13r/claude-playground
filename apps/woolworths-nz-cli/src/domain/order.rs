//! Past orders, once the `orders` response has been normalised.
//!
//! Only the list is modelled. The account the API was traced against had no
//! order history, so the operation the site uses for a single order's contents
//! was never captured and is not guessed at here -- see `wwnz orders --help`.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    /// Orders still in flight: placed, being picked, out for delivery.
    Active,
    /// Orders that are done with, which is what "history" means to a person.
    Past,
    All,
}

impl Filter {
    /// The `inclusiveFilter` value the API wants.
    pub fn wire(self) -> &'static str {
        match self {
            Filter::Active => "ACTIVE",
            Filter::Past => "PAST",
            Filter::All => "ALL",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Filter::Active => "active",
            Filter::Past => "past",
            Filter::All => "all",
        }
    }
}

impl std::str::FromStr for Filter {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Filter> {
        match s.trim().to_lowercase().as_str() {
            "active" | "open" | "current" => Ok(Filter::Active),
            "past" | "completed" | "history" => Ok(Filter::Past),
            "all" => Ok(Filter::All),
            other => anyhow::bail!("unknown order filter '{other}' (expected active, past or all)"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfilment_status: Option<String>,
    #[serde(rename = "total", serialize_with = "crate::domain::money::as_dollars")]
    pub total_cents: Option<i64>,
    /// "pickup" or "delivery".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// The store for a pickup, the address for a delivery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    /// When the slot starts, as the API spells it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_start: Option<String>,
    pub amendable: bool,
}

impl Order {
    /// The date part of an ISO timestamp, which is all a table row has room
    /// for. Anything that is not shaped like one is passed through whole.
    pub fn placed_on(&self) -> Option<String> {
        let raw = self.placed_at.as_deref()?;
        Some(match raw.split_once('T') {
            Some((date, _)) if date.len() == 10 => date.to_string(),
            _ => raw.to_string(),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OrderPage {
    pub orders: Vec<Order>,
    pub total: u32,
    pub total_pages: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(placed: Option<&str>) -> Order {
        Order {
            number: "12345678".into(),
            placed_at: placed.map(str::to_string),
            status: Some("PLACED".into()),
            fulfilment_status: None,
            total_cents: Some(2518),
            method: Some("pickup".into()),
            destination: Some("Regent Woolworths".into()),
            slot_start: None,
            amendable: true,
        }
    }

    #[test]
    fn a_placed_date_is_the_date_half_of_the_timestamp() {
        assert_eq!(
            order(Some("2026-09-02T14:30:00+12:00"))
                .placed_on()
                .as_deref(),
            Some("2026-09-02")
        );
        // Anything not shaped like a timestamp is shown as it came.
        assert_eq!(
            order(Some("2 Sep 2026")).placed_on().as_deref(),
            Some("2 Sep 2026")
        );
        assert_eq!(order(None).placed_on(), None);
    }

    #[test]
    fn filters_parse_the_words_people_use() {
        for (input, want) in [
            ("active", Filter::Active),
            ("OPEN", Filter::Active),
            ("past", Filter::Past),
            ("history", Filter::Past),
            ("all", Filter::All),
        ] {
            assert_eq!(input.parse::<Filter>().unwrap(), want, "{input}");
        }
        assert!("sideways".parse::<Filter>().is_err());
        assert_eq!(Filter::Past.wire(), "PAST");
    }
}
