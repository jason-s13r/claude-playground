//! Past shops.

use serde::{Deserialize, Serialize};

use crate::cart::Quantity;
use crate::money::as_dollars_opt;
use crate::retailer::RetailerId;
use crate::store::StoreRef;

/// Which orders to list. Foodstuffs splits by where the shop happened;
/// Woolworths splits by whether it is finished. Both are offered and each
/// adapter maps what it can -- see `Retailer::orders`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrderFilter {
    #[default]
    All,
    Active,
    Past,
    Online,
    InStore,
}

impl std::str::FromStr for OrderFilter {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(
            match s.trim().to_lowercase().replace(['_', '-'], "").as_str() {
                "all" => OrderFilter::All,
                "active" | "open" | "current" => OrderFilter::Active,
                "past" | "completed" | "history" => OrderFilter::Past,
                "online" | "web" | "delivery" => OrderFilter::Online,
                "instore" | "store" | "shop" | "till" => OrderFilter::InStore,
                _ => return Err(format!("unknown order filter {s:?}")),
            },
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderSummary {
    pub retailer: RetailerId,
    pub id: String,
    pub placed_at: Option<String>,
    #[serde(rename = "total", serialize_with = "as_dollars_opt")]
    pub total_cents: Option<i64>,
    pub status: Option<String>,
    pub fulfilment: Option<String>,
    pub store: Option<StoreRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderLine {
    pub sku: String,
    pub key: String,
    pub name: String,
    pub brand: Option<String>,
    pub quantity: Quantity,
    #[serde(rename = "total", serialize_with = "as_dollars_opt")]
    pub total_cents: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Order {
    #[serde(flatten)]
    pub summary: OrderSummary,
    pub lines: Vec<OrderLine>,
    pub address: Option<String>,
    pub timeslot: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub adjustments: Vec<crate::cart::Adjustment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_synonyms_both_retailers_suggest() {
        for (input, want) in [
            ("all", OrderFilter::All),
            ("open", OrderFilter::Active),
            ("current", OrderFilter::Active),
            ("history", OrderFilter::Past),
            ("in-store", OrderFilter::InStore),
            ("instore", OrderFilter::InStore),
            ("till", OrderFilter::InStore),
            ("delivery", OrderFilter::Online),
        ] {
            assert_eq!(input.parse::<OrderFilter>().unwrap(), want, "{input}");
        }
    }

    #[test]
    fn names_the_filter_it_could_not_read() {
        let err = "yesterday".parse::<OrderFilter>().unwrap_err();
        assert!(err.contains("yesterday"), "{err}");
    }
}
