//! What this crate hands back. Foodstuffs-shaped, on purpose.

use serde::Serialize;

use crate::banner::Banner;

#[derive(Clone, Debug, Serialize)]
pub struct Product {
    /// `5010819-EA-000`. Also the key cart mutations use.
    pub sku: String,
    pub banner: Banner,
    pub name: String,
    pub brand: Option<String>,
    /// The pack size, as printed.
    pub size: Option<String>,
    pub price_cents: Option<i64>,
    pub unit_price_cents: Option<i64>,
    pub unit_measure: Option<String>,
    /// Already rendered by the promotion: "2 for $5.00".
    pub multi_buy: Option<String>,
    pub is_special: bool,
    pub in_stock: Option<bool>,
    pub department: Option<String>,
    pub image: Option<String>,
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Store {
    pub id: String,
    pub name: String,
    pub banner: Banner,
    pub region: Option<String>,
    pub address: Option<String>,
}

/// A node of the department tree.
///
/// Store-scoped, and mixes promotional nodes ("Bonus Sticker Products",
/// "Father's Day") in with real departments. They are reported as they arrive:
/// classifying them would mean guessing, and the guess would go stale weekly.
#[derive(Clone, Debug, Serialize)]
pub struct Category {
    pub name: String,
    pub children: Vec<Category>,
}
