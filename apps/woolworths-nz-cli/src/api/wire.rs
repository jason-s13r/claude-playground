//! The shapes the GraphQL API answers with, and how they become domain types.
//!
//! Everything is optional on the way in. These are undocumented endpoints, so a
//! field Woolworths renames should degrade to a missing column rather than a
//! failed command.
//!
//! Prices arrive two ways and both are here. Search quotes dollars as a JSON
//! number (`7.19`); the cart and orders quote whole cents (`719`). Nothing
//! keeps floating-point money past the boundary -- [`cents`] converts on the
//! way in and everything downstream is integer cents.

use serde::Deserialize;

use crate::domain::cart::{Cart, CartLine};
use crate::domain::category::Category;
use crate::domain::order::{Order, OrderPage};
use crate::domain::{Product, Store};

/// Dollars as a float to exact cents.
///
/// Rounding rather than truncating: 7.19 is not representable in binary
/// floating point and arrives as 7.189999..., which truncation would turn into
/// $7.18.
pub fn cents(dollars: f64) -> i64 {
    (dollars * 100.0).round() as i64
}

// ---- product search ----

#[derive(Debug, Deserialize)]
pub struct SearchEnvelope {
    #[serde(rename = "My")]
    pub my: Option<My>,
}

#[derive(Debug, Deserialize)]
pub struct My {
    pub products: Option<ProductPage>,
    pub categories: Option<WireCategory>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductPage {
    #[serde(default)]
    pub results: Vec<WireResult>,
    pub total_count: Option<u32>,
    pub total_pages: Option<u32>,
}

/// One row of a result page.
///
/// The results list is a union: as well as products it carries ad slots,
/// editorial tiles and redirect hints. Only the two product-shaped members are
/// named, and `#[serde(other)]` swallows the rest -- an unfamiliar member is a
/// row to skip, not a parse failure.
#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
pub enum WireResult {
    ProductSummary(Box<WireProduct>),
    SponsoredProduct(Box<WireProduct>),
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireProduct {
    pub sku: Option<String>,
    pub product_name: Option<String>,
    pub brand: Option<String>,
    pub slug: Option<String>,
    pub image_url: Option<String>,
    pub store_key: Option<String>,
    #[serde(default)]
    pub variants: Vec<WireVariant>,
    pub category_hierarchy_names: Option<Hierarchy>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVariant {
    pub variant_key: Option<String>,
    pub unit_of_measure: Option<String>,
    pub availability_status: Option<String>,
    pub variant_price: Option<WirePrice>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePrice {
    pub selling_price: Option<f64>,
    pub was_price: Option<f64>,
    pub cup_price: Option<f64>,
    pub cup_unit: Option<String>,
    pub is_special: Option<bool>,
    pub is_club_price: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Hierarchy {
    #[serde(default)]
    pub lvl1: Vec<String>,
}

impl WireProduct {
    /// The product as one row, priced from its first variant.
    ///
    /// Nearly everything has exactly one variant; where there are more, the
    /// site leads with the first and so does this.
    pub fn into_product(self, origin: &str, sponsored: bool) -> Option<Product> {
        let sku = self.sku?;
        let variant = self.variants.into_iter().next();
        let price = variant.as_ref().and_then(|v| v.variant_price.as_ref());
        let availability = variant.as_ref().and_then(|v| v.availability_status.clone());
        let slug = self.slug.unwrap_or_default();

        Some(Product {
            url: format!("{origin}/shop/product-details/{sku}/{slug}"),
            variant_key: variant
                .as_ref()
                .and_then(|v| v.variant_key.clone())
                // The cart is keyed by variant, so a product whose variant key
                // did not come back still needs the conventional one.
                .unwrap_or_else(|| format!("{sku}-EA")),
            name: self.product_name.unwrap_or_default(),
            brand: self.brand.filter(|b| !b.trim().is_empty()),
            unit_of_measure: variant.as_ref().and_then(|v| v.unit_of_measure.clone()),
            price_cents: price.and_then(|p| p.selling_price).map(cents),
            was_price_cents: price.and_then(|p| p.was_price).map(cents),
            unit_price_cents: price.and_then(|p| p.cup_price).map(cents),
            unit_measure: price
                .and_then(|p| p.cup_unit.clone())
                .filter(|u| !u.trim().is_empty()),
            is_special: price.and_then(|p| p.is_special).unwrap_or(false),
            is_club_price: price.and_then(|p| p.is_club_price).unwrap_or(false),
            in_stock: availability.as_deref().map(|a| {
                !a.eq_ignore_ascii_case("UNAVAILABLE") && !a.eq_ignore_ascii_case("OUT_OF_STOCK")
            }),
            availability,
            department: self
                .category_hierarchy_names
                .and_then(|h| h.lvl1.into_iter().next()),
            store_key: self.store_key,
            sponsored,
            image: self.image_url,
            sku,
        })
    }
}

// ---- categories ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCategory {
    pub key: Option<String>,
    pub name: Option<String>,
    pub display_slug: Option<String>,
    pub slug: Option<String>,
    pub level: Option<u32>,
    #[serde(default)]
    pub children: Vec<WireCategory>,
}

impl WireCategory {
    pub fn into_category(self) -> Option<Category> {
        Some(Category {
            key: self.key?,
            name: self.name.unwrap_or_default(),
            slug: self.display_slug.or(self.slug).unwrap_or_default(),
            level: self.level.unwrap_or(0),
            children: self
                .children
                .into_iter()
                .filter_map(WireCategory::into_category)
                .collect(),
        })
    }
}

// ---- stores ----

#[derive(Debug, Deserialize)]
pub struct LocationsEnvelope {
    pub locations: Option<LocationsResult>,
}

#[derive(Debug, Deserialize)]
pub struct LocationsResult {
    #[serde(default)]
    pub locations: Vec<WireLocation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireLocation {
    pub id: Option<String>,
    pub store_id: Option<String>,
    pub name: Option<String>,
    pub distance: Option<f64>,
    pub address: Option<WireAddress>,
}

#[derive(Debug, Deserialize)]
pub struct WireAddress {
    pub lines: Option<WireLines>,
    pub locality: Option<WireLocality>,
}

#[derive(Debug, Deserialize)]
pub struct WireLines {
    pub line1: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WireLocality {
    pub suburb: Option<String>,
    pub city: Option<String>,
}

impl WireLocation {
    pub fn into_store(self) -> Option<Store> {
        let locality = self.address.as_ref().and_then(|a| a.locality.as_ref());
        Some(Store {
            id: self.store_id.or(self.id)?,
            name: self.name.unwrap_or_else(|| "(unnamed)".into()),
            address: self
                .address
                .as_ref()
                .and_then(|a| a.lines.as_ref())
                .and_then(|l| l.line1.clone())
                .filter(|l| !l.trim().is_empty()),
            suburb: locality
                .and_then(|l| l.suburb.clone())
                .filter(|s| !s.trim().is_empty()),
            city: locality
                .and_then(|l| l.city.clone())
                .filter(|s| !s.trim().is_empty()),
            distance_km: self.distance,
        })
    }
}

// ---- cart ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCart {
    pub key: Option<String>,
    /// Counts quantities, so a weighed line makes it fractional too.
    pub total_item_quantity: Option<f64>,
    pub total_unique_product_sku: Option<u32>,
    #[serde(default)]
    pub line_items: Vec<WireLineItem>,
    pub checkout: Option<WireCheckout>,
    pub pricing: Option<WirePricing>,
    pub validation_result: Option<WireValidation>,
    pub shopping_mode: Option<WireShoppingMode>,
    pub fulfilment: Option<WireFulfilment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCheckout {
    pub amount_to_pay_as_cents: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePricing {
    /// Products plus fees. This is what the site calls the order subtotal, and
    /// it is not the sum of the line totals.
    pub order_subtotal: Option<WireEvaluated>,
    /// The lines alone, which is what does sum to the rows on screen.
    pub product_subtotal: Option<WireEvaluated>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireEvaluated {
    pub after_discount_as_cents: Option<i64>,
    pub discount_amount_as_cents: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireValidation {
    #[serde(default)]
    pub failed_validations: Vec<WireFailure>,
}

#[derive(Debug, Deserialize)]
pub struct WireFailure {
    pub message: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireLineItem {
    pub sku: Option<String>,
    pub product_variant_sku: Option<String>,
    /// Kilograms on a `-KGM` line, a count on every other one.
    pub quantity: Option<f64>,
    pub can_substitute: Option<bool>,
    pub line_total: Option<WireEvaluated>,
    pub unit_price: Option<WireEvaluated>,
    pub product: Option<WireCartProduct>,
}

#[derive(Debug, Deserialize)]
pub struct WireCartProduct {
    pub brand: Option<String>,
    #[serde(default)]
    pub variants: Vec<WireCartVariant>,
}

#[derive(Debug, Deserialize)]
pub struct WireCartVariant {
    pub name: Option<String>,
    pub key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireShoppingMode {
    pub pickup_location: Option<WireNamed>,
}

#[derive(Debug, Deserialize)]
pub struct WireNamed {
    pub id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireFulfilment {
    pub fulfilment_proposition: Option<WireProposition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireProposition {
    pub store_id: Option<String>,
    pub method: Option<String>,
    pub store: Option<WireStoreRef>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireStoreRef {
    pub store_id: Option<String>,
    pub name: Option<String>,
}

impl WireCart {
    pub fn into_cart(self) -> Cart {
        let pricing = self.pricing.as_ref();
        let subtotal = pricing.and_then(|p| p.order_subtotal.as_ref());
        let products = pricing.and_then(|p| p.product_subtotal.as_ref());
        let proposition = self
            .fulfilment
            .as_ref()
            .and_then(|f| f.fulfilment_proposition.as_ref());
        let pickup = self
            .shopping_mode
            .as_ref()
            .and_then(|s| s.pickup_location.as_ref());

        let lines: Vec<CartLine> = self
            .line_items
            .into_iter()
            .filter_map(WireLineItem::into_line)
            .collect();

        Cart {
            id: self.key,
            total_items: self.total_item_quantity.unwrap_or(lines.len() as f64),
            unique_products: self.total_unique_product_sku.unwrap_or(lines.len() as u32),
            subtotal_cents: subtotal.and_then(|s| s.after_discount_as_cents),
            items_cents: products.and_then(|s| s.after_discount_as_cents),
            to_pay_cents: self.checkout.and_then(|c| c.amount_to_pay_as_cents),
            discount_cents: subtotal
                .and_then(|s| s.discount_amount_as_cents)
                .filter(|c| *c != 0),
            // The proposition names the store it will be picked from, which is
            // the one that matters; the pickup location is the fallback for a
            // cart with no slot chosen yet.
            store_name: proposition
                .and_then(|p| p.store.as_ref())
                .and_then(|s| s.name.clone())
                .or_else(|| pickup.and_then(|p| p.name.clone())),
            store_id: proposition
                .and_then(|p| p.store.as_ref())
                .and_then(|s| s.store_id.clone())
                .or_else(|| proposition.and_then(|p| p.store_id.clone()))
                .or_else(|| pickup.and_then(|p| p.id.clone())),
            fulfilment_method: proposition.and_then(|p| p.method.clone()),
            problems: self
                .validation_result
                .map(|v| {
                    v.failed_validations
                        .into_iter()
                        .filter_map(|f| f.message.or(f.title))
                        .collect()
                })
                .unwrap_or_default(),
            lines,
        }
    }
}

impl WireLineItem {
    fn into_line(self) -> Option<CartLine> {
        let variant = self.product.as_ref().and_then(|p| p.variants.first());
        let variant_key = self
            .product_variant_sku
            .clone()
            .or_else(|| variant.and_then(|v| v.key.clone()))?;
        Some(CartLine {
            sku: self
                .sku
                // A line with no SKU of its own still has one inside its
                // variant key, which is `<sku>-<unit>`.
                .or_else(|| variant_key.split('-').next().map(str::to_string))
                .unwrap_or_default(),
            name: variant.and_then(|v| v.name.clone()).unwrap_or_default(),
            brand: self
                .product
                .as_ref()
                .and_then(|p| p.brand.clone())
                .filter(|b| !b.trim().is_empty()),
            quantity: self.quantity.unwrap_or(0.0),
            unit_price_cents: self
                .unit_price
                .as_ref()
                .and_then(|p| p.after_discount_as_cents),
            total_cents: self
                .line_total
                .as_ref()
                .and_then(|t| t.after_discount_as_cents),
            discount_cents: self
                .line_total
                .as_ref()
                .and_then(|t| t.discount_amount_as_cents)
                .filter(|c| *c != 0),
            can_substitute: self.can_substitute.unwrap_or(false),
            variant_key,
        })
    }
}

// ---- orders ----

#[derive(Debug, Deserialize)]
pub struct OrdersEnvelope {
    pub orders: Option<WireOrderPage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireOrderPage {
    #[serde(default)]
    pub results: Vec<WireOrder>,
    pub total_count: Option<u32>,
    pub total_pages: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireOrder {
    pub order_number: Option<String>,
    pub created_date_time: Option<String>,
    pub order_status: Option<String>,
    pub fulfilment_status: Option<String>,
    pub is_amendable: Option<bool>,
    pub total: Option<WireOrderTotal>,
    #[serde(default)]
    pub fulfilments: Vec<WireOrderFulfilment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireOrderTotal {
    pub after_discount_in_cents: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireOrderFulfilment {
    pub method: Option<String>,
    pub start_time: Option<String>,
    pub fulfilment_location: Option<WireNamed>,
    pub address: Option<WireOrderAddress>,
}

#[derive(Debug, Deserialize)]
pub struct WireOrderAddress {
    pub lines: Option<WireLines>,
}

impl WireOrderPage {
    pub fn into_page(self) -> OrderPage {
        let orders: Vec<Order> = self
            .results
            .into_iter()
            .filter_map(WireOrder::into_order)
            .collect();
        OrderPage {
            total: self.total_count.unwrap_or(orders.len() as u32),
            total_pages: self.total_pages.unwrap_or(1),
            orders,
        }
    }
}

impl WireOrder {
    fn into_order(self) -> Option<Order> {
        let fulfilment = self.fulfilments.into_iter().next();
        Some(Order {
            number: self.order_number?,
            placed_at: self.created_date_time,
            status: self.order_status,
            fulfilment_status: self.fulfilment_status,
            total_cents: self.total.and_then(|t| t.after_discount_in_cents),
            method: fulfilment.as_ref().and_then(|f| f.method.clone()),
            // A pickup names a store; a delivery names an address.
            destination: fulfilment.as_ref().and_then(|f| {
                f.fulfilment_location
                    .as_ref()
                    .and_then(|l| l.name.clone())
                    .or_else(|| {
                        f.address
                            .as_ref()
                            .and_then(|a| a.lines.as_ref())
                            .and_then(|l| l.line1.clone())
                    })
            }),
            slot_start: fulfilment.and_then(|f| f.start_time),
            amendable: self.is_amendable.unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dollar_prices_become_exact_cents() {
        // 7.19 is not representable in binary floating point, so truncating
        // here would quietly bill a cent short.
        assert_eq!(cents(7.19), 719);
        assert_eq!(cents(4.82), 482);
        assert_eq!(cents(0.05), 5);
        assert_eq!(cents(10.0), 1000);
    }

    #[test]
    fn unknown_union_members_are_skipped_rather_than_fatal() {
        let json = serde_json::json!([
            { "__typename": "GamResultItem", "adUnitApp": "x" },
            { "__typename": "RedirectResultItem", "url": "/somewhere" },
            { "__typename": "ProductSummary", "sku": "1", "productName": "Milk" },
        ]);
        let results: Vec<WireResult> =
            serde_json::from_value(json).expect("ad slots are not errors");
        assert!(matches!(results[0], WireResult::Other));
        assert!(matches!(results[1], WireResult::Other));
        assert!(matches!(results[2], WireResult::ProductSummary(_)));
    }

    #[test]
    fn a_product_missing_everything_optional_still_maps() {
        let wire: WireProduct =
            serde_json::from_value(serde_json::json!({ "sku": "282768" })).unwrap();
        let p = wire.into_product("https://example.test", false).unwrap();
        assert_eq!(p.sku, "282768");
        // No variant came back, so the conventional key is used.
        assert_eq!(p.variant_key, "282768-EA");
        assert_eq!(p.price_cents, None);
        assert_eq!(p.in_stock, None);
    }

    #[test]
    fn a_product_with_no_sku_is_dropped() {
        let wire: WireProduct =
            serde_json::from_value(serde_json::json!({ "productName": "Milk" })).unwrap();
        assert!(wire.into_product("https://example.test", false).is_none());
    }

    #[test]
    fn a_cart_line_recovers_its_sku_from_the_variant_key() {
        let wire: WireLineItem = serde_json::from_value(serde_json::json!({
            "productVariantSku": "282768-EA",
            "quantity": 2,
        }))
        .unwrap();
        let line = wire.into_line().unwrap();
        assert_eq!(line.sku, "282768");
        assert_eq!(line.variant_key, "282768-EA");
        assert_eq!(line.quantity, 2.0);
    }

    #[test]
    fn a_weighed_line_keeps_its_fractional_quantity() {
        // Loose produce is priced by the kilogram, so 300g of it arrives as
        // `0.3`. Reading that as a count is what made `cart list` fail
        // outright on any cart holding one.
        let wire: WireLineItem = serde_json::from_value(serde_json::json!({
            "productVariantSku": "133211-KGM",
            "quantity": 0.3,
        }))
        .unwrap();
        let line = wire.into_line().unwrap();
        assert_eq!(line.quantity, 0.3);

        let cart: WireCart = serde_json::from_value(serde_json::json!({
            "totalItemQuantity": 1.3,
            "lineItems": [
                { "productVariantSku": "133211-KGM", "quantity": 0.3 },
                { "productVariantSku": "282768-EA", "quantity": 1 },
            ],
        }))
        .unwrap();
        let cart = cart.into_cart();
        assert_eq!(cart.total_items, 1.3);
        assert_eq!(cart.lines[0].quantity, 0.3);
        assert_eq!(cart.lines[1].quantity, 1.0);
    }

    #[test]
    fn an_empty_cart_reports_zeroes_rather_than_failing() {
        let wire: WireCart = serde_json::from_value(serde_json::json!({})).unwrap();
        let cart = wire.into_cart();
        assert!(cart.is_empty());
        assert_eq!(cart.total_items, 0.0);
        assert!(cart.problems.is_empty());
    }

    #[test]
    fn a_zero_discount_is_not_shown_as_a_discount() {
        let wire: WireCart = serde_json::from_value(serde_json::json!({
            "pricing": { "orderSubtotal": {
                "afterDiscountAsCents": 2518, "discountAmountAsCents": 0
            }}
        }))
        .unwrap();
        let cart = wire.into_cart();
        assert_eq!(cart.subtotal_cents, Some(2518));
        assert_eq!(cart.discount_cents, None);
    }

    #[test]
    fn a_pickup_order_names_its_store_and_a_delivery_its_address() {
        let pickup: WireOrder = serde_json::from_value(serde_json::json!({
            "orderNumber": "1",
            "fulfilments": [{ "method": "pickup", "fulfilmentLocation": { "name": "Regent Woolworths" } }],
        }))
        .unwrap();
        assert_eq!(
            pickup.into_order().unwrap().destination.as_deref(),
            Some("Regent Woolworths")
        );

        let delivery: WireOrder = serde_json::from_value(serde_json::json!({
            "orderNumber": "2",
            "fulfilments": [{ "method": "delivery", "address": { "lines": { "line1": "13 Oranga Road" } } }],
        }))
        .unwrap();
        assert_eq!(
            delivery.into_order().unwrap().destination.as_deref(),
            Some("13 Oranga Road")
        );
    }
}
