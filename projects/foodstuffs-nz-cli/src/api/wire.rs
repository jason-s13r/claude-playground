//! The shapes Foodstuffs actually sends back.
//!
//! Every field is optional on purpose: these are undocumented endpoints, and a
//! response that drops a field should narrow what we can show, not fail the
//! command. Turning these into the crate's own types is `Client`'s job.

use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct WireStore {
    pub id: Option<String>,
    pub name: Option<String>,
    pub region: Option<String>,
    pub address: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WireSearchPage {
    pub products: Option<Vec<WireProduct>>,
    pub total_hits: Option<u32>,
    pub total_pages: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WireProduct {
    pub product_id: Option<String>,
    pub name: Option<String>,
    pub brand: Option<String>,
    /// The pack size, despite the name.
    pub display_name: Option<String>,
    pub availability: Option<Vec<String>>,
    pub single_price: Option<WireSinglePrice>,
    pub promotions: Option<Vec<WirePromotion>>,
    pub category_trees: Option<Vec<WireCategoryTree>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WireSinglePrice {
    /// Cents.
    pub price: Option<i64>,
    pub promo_id: Option<serde_json::Value>,
    pub comparative_price: Option<WireComparativePrice>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WireComparativePrice {
    /// Cents.
    pub price_per_unit: Option<i64>,
    pub measure_description: Option<String>,
    pub unit_quantity_uom: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WirePromotion {
    pub threshold: Option<u32>,
    /// Cents.
    pub reward_value: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WireCategoryTree {
    pub level0: Option<String>,
}
