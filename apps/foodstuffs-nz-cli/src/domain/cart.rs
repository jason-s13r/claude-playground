//! The shopping cart.
//!
//! Unlike search, this needs a real account -- `fsnz auth login` -- because the cart
//! belongs to a person rather than a store. Money is in cents throughout, and a
//! line's `price` is the line total, not the unit price.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::domain::dollars;

/// How a product is sold. Loose produce is priced by weight and its quantity is
/// carried in grams; everything else is counted.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SaleType {
    Units,
    Weight,
}

impl SaleType {
    /// Foodstuffs encodes this in the SKU: `-KGM-` is sold by the kilogram,
    /// `-EA-` by the each. Inferring it means nobody has to pass `--unit` for
    /// the ordinary case.
    pub fn infer(sku: &str) -> SaleType {
        if sku.to_ascii_uppercase().contains("-KGM-") {
            SaleType::Weight
        } else {
            SaleType::Units
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            SaleType::Units => "UNITS",
            SaleType::Weight => "WEIGHT",
        }
    }

    pub fn parse(s: &str) -> Option<SaleType> {
        match s.trim().to_ascii_lowercase().as_str() {
            "units" | "unit" | "each" | "ea" => Some(SaleType::Units),
            "weight" | "grams" | "g" | "kg" => Some(SaleType::Weight),
            _ => None,
        }
    }

    /// Render a quantity the way a person would say it.
    pub fn quantity_label(self, quantity: u32) -> String {
        match self {
            SaleType::Units => format!("{quantity}"),
            SaleType::Weight if quantity >= 1000 && quantity.is_multiple_of(1000) => {
                format!("{}kg", quantity / 1000)
            }
            SaleType::Weight => format!("{quantity}g"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CartItem {
    pub sku: String,
    pub name: String,
    pub quantity: u32,
    pub sale_type: SaleType,
    #[serde(rename = "line_total")]
    pub line_total_cents: i64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_liquor: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

impl CartItem {
    pub fn quantity_label(&self) -> String {
        self.sale_type.quantity_label(self.quantity)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Cart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_name: Option<String>,
    pub items: Vec<CartItem>,
    /// Things in the cart the store cannot currently supply.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unavailable: Vec<CartItem>,
    pub subtotal_cents: i64,
    pub service_fee_cents: i64,
    pub bag_fee_cents: i64,
    pub promo_discount_cents: i64,
    pub subscription_discount_cents: i64,
    pub club_member: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priced_at: Option<String>,
}

impl Cart {
    /// Subtotal plus fees, less discounts. Not a quote -- the real total is
    /// settled at checkout, which this tool deliberately does not do.
    pub fn estimated_total_cents(&self) -> i64 {
        self.subtotal_cents + self.service_fee_cents + self.bag_fee_cents
            - self.promo_discount_cents
            - self.subscription_discount_cents
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn summary(&self) -> String {
        format!(
            "{} line{}, {} estimated",
            self.item_count(),
            crate::output::plural(self.item_count()),
            dollars(self.estimated_total_cents())
        )
    }
}

// ---- wire shapes ---------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCart {
    products: Option<Vec<WireCartItem>>,
    unavailable_products: Option<Vec<WireCartItem>>,
    subtotal: Option<i64>,
    service_fee: Option<i64>,
    bag_fee: Option<i64>,
    promo_code_discount: Option<i64>,
    subscription_discount: Option<i64>,
    club_member: Option<bool>,
    when_last_priced: Option<String>,
    store: Option<WireCartStore>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireCartStore {
    store_id: Option<String>,
    store_name: Option<String>,
}

#[derive(Deserialize)]
struct WireCartItem {
    #[serde(rename = "productId")]
    product_id: Option<String>,
    name: Option<String>,
    quantity: Option<f64>,
    sale_type: Option<String>,
    price: Option<i64>,
    #[serde(rename = "isLiquor")]
    is_liquor: Option<bool>,
    #[serde(rename = "originStatement")]
    origin_statement: Option<String>,
}

impl WireCart {
    pub fn into_cart(self) -> Cart {
        let convert = |items: Option<Vec<WireCartItem>>| -> Vec<CartItem> {
            items
                .unwrap_or_default()
                .into_iter()
                .map(|i| {
                    let sku = i.product_id.unwrap_or_default();
                    let sale_type = i
                        .sale_type
                        .as_deref()
                        .and_then(SaleType::parse)
                        .unwrap_or_else(|| SaleType::infer(&sku));
                    CartItem {
                        sku,
                        name: i.name.unwrap_or_default(),
                        quantity: i.quantity.unwrap_or(0.0).max(0.0).round() as u32,
                        sale_type,
                        line_total_cents: i.price.unwrap_or(0),
                        is_liquor: i.is_liquor.unwrap_or(false),
                        origin: i.origin_statement.filter(|o| !o.trim().is_empty()),
                    }
                })
                .collect()
        };
        let store = self.store;
        Cart {
            store_id: store.as_ref().and_then(|s| s.store_id.clone()),
            store_name: store.as_ref().and_then(|s| s.store_name.clone()),
            items: convert(self.products),
            unavailable: convert(self.unavailable_products),
            subtotal_cents: self.subtotal.unwrap_or(0),
            service_fee_cents: self.service_fee.unwrap_or(0),
            bag_fee_cents: self.bag_fee.unwrap_or(0),
            promo_discount_cents: self.promo_code_discount.unwrap_or(0),
            subscription_discount_cents: self.subscription_discount.unwrap_or(0),
            club_member: self.club_member.unwrap_or(false),
            priced_at: self.when_last_priced,
        }
    }
}

/// One change to apply to the cart. A quantity of zero removes the line.
#[derive(Clone, Debug)]
pub struct Change {
    pub sku: String,
    pub quantity: u32,
    pub sale_type: SaleType,
}

impl Change {
    pub fn wire(&self) -> serde_json::Value {
        serde_json::json!({
            "productId": self.sku,
            "quantity": self.quantity,
            "sale_type": self.sale_type.wire(),
        })
    }
}

pub fn changes_body(changes: &[Change]) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "products": changes.iter().map(Change::wire).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sale_type_comes_from_the_sku_without_being_asked() {
        assert_eq!(SaleType::infer("5039956-EA-000"), SaleType::Units);
        assert_eq!(SaleType::infer("5101189-KGM-000"), SaleType::Weight);
        // Unknown shapes count rather than weigh, which is the safer default.
        assert_eq!(SaleType::infer("weird-sku"), SaleType::Units);
    }

    #[test]
    fn quantities_read_the_way_people_say_them() {
        assert_eq!(SaleType::Units.quantity_label(2), "2");
        assert_eq!(SaleType::Weight.quantity_label(300), "300g");
        assert_eq!(SaleType::Weight.quantity_label(1000), "1kg");
        assert_eq!(SaleType::Weight.quantity_label(1500), "1500g");
    }

    #[test]
    fn the_unit_flag_accepts_what_people_type() {
        for s in ["units", "each", "EA", "Unit"] {
            assert_eq!(SaleType::parse(s), Some(SaleType::Units), "{s}");
        }
        for s in ["weight", "kg", "grams"] {
            assert_eq!(SaleType::parse(s), Some(SaleType::Weight), "{s}");
        }
        assert_eq!(SaleType::parse("litres"), None);
    }

    #[test]
    fn the_change_body_matches_what_the_site_sends() {
        let body = changes_body(&[Change {
            sku: "5101189-KGM-000".into(),
            quantity: 300,
            sale_type: SaleType::Weight,
        }])
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"products":[
                {"productId":"5101189-KGM-000","quantity":300,"sale_type":"WEIGHT"}
            ]})
        );
    }

    #[test]
    fn totals_add_the_fees_and_take_off_the_discounts() {
        let cart = Cart {
            store_id: None,
            store_name: None,
            items: vec![],
            unavailable: vec![],
            subtotal_cents: 1807,
            service_fee_cents: 0,
            bag_fee_cents: 150,
            promo_discount_cents: 100,
            subscription_discount_cents: 7,
            club_member: true,
            priced_at: None,
        };
        assert_eq!(cart.estimated_total_cents(), 1807 + 150 - 100 - 7);
    }
}
