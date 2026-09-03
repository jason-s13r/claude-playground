//! The shopping cart.
//!
//! Unlike search this needs a real account: the cart belongs to a person rather
//! than a store. Money is in cents throughout, and a line's `price` is the line
//! total, not the unit price.

use serde::{Deserialize, Serialize};

/// How a product is sold. Loose produce is priced by weight and its quantity
/// travels in **grams**; everything else is counted.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SaleType {
    Units,
    Weight,
}

impl SaleType {
    /// Foodstuffs encodes this in the SKU: `-KGM-` is sold by the kilogram,
    /// `-EA-` by the each. Inferring it means nobody has to say so for the
    /// ordinary case.
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
    /// Units, or grams for a weight line.
    pub quantity: u32,
    pub sale_type: SaleType,
    pub line_total_cents: i64,
    pub is_liquor: bool,
    pub origin: Option<String>,
}

impl CartItem {
    pub fn quantity_label(&self) -> String {
        self.sale_type.quantity_label(self.quantity)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Cart {
    pub store_id: Option<String>,
    pub store_name: Option<String>,
    pub items: Vec<CartItem>,
    /// Things in the cart this store cannot currently supply. Kept apart
    /// because leaving them in `items` would make the totals lie.
    pub unavailable: Vec<CartItem>,
    pub subtotal_cents: i64,
    pub service_fee_cents: i64,
    pub bag_fee_cents: i64,
    pub promo_discount_cents: i64,
    pub subscription_discount_cents: i64,
    pub club_member: bool,
    pub priced_at: Option<String>,
}

impl Cart {
    /// Subtotal plus fees, less discounts.
    ///
    /// Not a quote: the real total is settled at checkout, which this
    /// deliberately does not do.
    pub fn estimated_total_cents(&self) -> i64 {
        self.subtotal_cents + self.service_fee_cents + self.bag_fee_cents
            - self.promo_discount_cents
            - self.subscription_discount_cents
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// One change to apply. A quantity of zero removes the line, which is how the
/// site does it too.
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

pub fn changes_body(changes: &[Change]) -> serde_json::Value {
    serde_json::json!({
        "products": changes.iter().map(Change::wire).collect::<Vec<_>>(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sku_says_whether_something_is_sold_by_weight() {
        assert_eq!(SaleType::infer("5010819-EA-000"), SaleType::Units);
        assert_eq!(SaleType::infer("5039180-KGM-000"), SaleType::Weight);
        assert_eq!(SaleType::infer("5039180-kgm-000"), SaleType::Weight);
    }

    #[test]
    fn weight_quantities_read_as_grams_or_whole_kilograms() {
        assert_eq!(SaleType::Weight.quantity_label(500), "500g");
        assert_eq!(SaleType::Weight.quantity_label(1000), "1kg");
        assert_eq!(SaleType::Weight.quantity_label(2000), "2kg");
        assert_eq!(SaleType::Weight.quantity_label(1500), "1500g");
        assert_eq!(SaleType::Units.quantity_label(2), "2");
    }

    #[test]
    fn a_change_carries_the_sale_type_the_api_expects() {
        let body = changes_body(&[Change {
            sku: "A-EA-000".into(),
            quantity: 0,
            sale_type: SaleType::Units,
        }]);
        assert_eq!(body["products"][0]["productId"], "A-EA-000");
        assert_eq!(body["products"][0]["quantity"], 0, "zero removes the line");
        assert_eq!(body["products"][0]["sale_type"], "UNITS");
    }

    #[test]
    fn an_explicit_sale_type_beats_the_one_the_sku_implies() {
        let raw = serde_json::json!({
            "products": [{ "productId": "A-EA-000", "sale_type": "WEIGHT", "quantity": 250.0 }]
        });
        let cart = serde_json::from_value::<WireCart>(raw).unwrap().into_cart();
        assert_eq!(cart.items[0].sale_type, SaleType::Weight);
        assert_eq!(cart.items[0].quantity_label(), "250g");
    }

    #[test]
    fn unavailable_lines_stay_out_of_the_totals() {
        let raw = serde_json::json!({
            "products": [{ "productId": "A-EA-000", "quantity": 1.0, "price": 500 }],
            "unavailableProducts": [{ "productId": "B-EA-000", "quantity": 1.0, "price": 400 }],
            "subtotal": 500,
            "serviceFee": 150,
            "bagFee": 25,
            "promoCodeDiscount": 100,
            "subscriptionDiscount": 50
        });
        let cart = serde_json::from_value::<WireCart>(raw).unwrap().into_cart();
        assert_eq!(cart.items.len(), 1);
        assert_eq!(cart.unavailable.len(), 1);
        assert_eq!(cart.estimated_total_cents(), 500 + 150 + 25 - 100 - 50);
    }

    #[test]
    fn a_cart_with_nothing_in_it_still_parses() {
        let cart = serde_json::from_value::<WireCart>(serde_json::json!({}))
            .unwrap()
            .into_cart();
        assert!(cart.is_empty());
        assert_eq!(cart.estimated_total_cents(), 0);
    }

    #[test]
    fn a_fractional_quantity_is_rounded_rather_than_truncated() {
        let raw = serde_json::json!({
            "products": [{ "productId": "A-KGM-000", "quantity": 249.6 }]
        });
        let cart = serde_json::from_value::<WireCart>(raw).unwrap().into_cart();
        assert_eq!(cart.items[0].quantity, 250);
    }
}
