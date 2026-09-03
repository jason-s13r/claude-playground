//! The shapes Foodstuffs actually sends back.
//!
//! Every field is optional on purpose: these are undocumented endpoints, and a
//! response that drops a field should narrow what can be shown, not fail the
//! command. Turning these into this crate's types is the client's job.

use serde::Deserialize;

#[derive(Deserialize)]
pub struct WireStore {
    pub id: Option<String>,
    pub name: Option<String>,
    pub region: Option<String>,
    pub address: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSearchPage {
    pub products: Option<Vec<WireProduct>>,
    pub total_hits: Option<u32>,
    pub total_pages: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireProduct {
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
pub struct WireSinglePrice {
    /// Cents.
    pub price: Option<i64>,
    pub promo_id: Option<serde_json::Value>,
    pub comparative_price: Option<WireComparativePrice>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireComparativePrice {
    /// Cents.
    pub price_per_unit: Option<i64>,
    pub measure_description: Option<String>,
    pub unit_quantity_uom: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePromotion {
    pub threshold: Option<u32>,
    /// Cents.
    pub reward_value: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCategoryTree {
    pub level0: Option<String>,
}

/// A node of `GET /v1/edge/store/{id}/categories`.
///
/// Carries a name and its children and nothing else -- no key, no slug -- which
/// is why lookups against this tree match on name. `appContent` is promotional
/// artwork and is ignored.
#[derive(Deserialize)]
pub struct WireCategory {
    pub name: Option<String>,
    #[serde(default)]
    pub children: Vec<WireCategory>,
}
