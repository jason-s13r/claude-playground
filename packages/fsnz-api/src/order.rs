//! Past orders.
//!
//! Two kinds, served from two endpoints: orders placed online, and till
//! receipts from shopping in a store, which are linked to the account through
//! Club Plus. Money is in cents, and a line's price is the line total.

use serde::{Deserialize, Serialize};

use crate::cart::SaleType;

/// Where an order came from. The list filters on it, and it decides which
/// endpoint one order's detail is read from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    InStore,
    Online,
}

impl Source {
    pub fn wire(self) -> &'static str {
        match self {
            Source::InStore => "IN_STORE",
            Source::Online => "ONLINE",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::InStore => "in store",
            Source::Online => "online",
        }
    }

    pub fn parse(s: &str) -> Option<Source> {
        let key: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match key.as_str() {
            "instore" | "store" | "shop" | "till" => Some(Source::InStore),
            "online" | "web" | "delivery" => Some(Source::Online),
            _ => None,
        }
    }

    /// In-store ids are paths -- `region/fsni/banner/NW/customer/...` -- and
    /// online ones a single opaque segment. That is enough to route an id
    /// somebody pasted without listing the history first.
    pub fn infer(order_id: &str) -> Source {
        if order_id.contains('/') {
            Source::InStore
        } else {
            Source::Online
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OrderSummary {
    pub id: String,
    pub placed_at: Option<String>,
    pub total_cents: Option<i64>,
    pub source: Option<Source>,
    pub store_name: Option<String>,
    pub store_id: Option<String>,
}

impl OrderSummary {
    /// The source the API reported, or the one the id implies.
    pub fn resolved_source(&self) -> Source {
        self.source.unwrap_or_else(|| Source::infer(&self.id))
    }
}

/// One product on an order. Also the shape `previousPurchases` returns.
#[derive(Clone, Debug, Serialize)]
pub struct OrderLine {
    pub sku: String,
    pub name: String,
    pub brand: Option<String>,
    pub quantity: u32,
    pub sale_type: SaleType,
    pub line_total_cents: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub summary: OrderSummary,
    pub lines: Vec<OrderLine>,
    /// The fields below only ever arrive on an online order; a till receipt
    /// carries none of them.
    pub status: Option<String>,
    pub fulfilment: Option<String>,
    pub address: Option<String>,
    pub timeslot: Option<String>,
    pub service_fee_cents: i64,
    pub bag_fee_cents: i64,
}

pub struct OrderPage {
    pub orders: Vec<OrderSummary>,
    pub total: u32,
    pub total_pages: u32,
}

// ---- wire shapes ---------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireOrderPage {
    page_info: Option<WirePageInfo>,
    orders: Option<Vec<WireOrderSummary>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePageInfo {
    total_pages: Option<u32>,
    total_contents_count: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireOrderSummary {
    order_id: Option<String>,
    /// A till receipt calls the total `amount`; an online order spells the same
    /// number one of the longer ways. All three are accepted.
    amount: Option<i64>,
    order_amount_in_cents: Option<i64>,
    total_cost_in_cents: Option<i64>,
    order_timestamp: Option<String>,
    source: Option<String>,
    status: Option<String>,
    /// Spelled with the extra 'l' by the API, not by us.
    fullfilment_method: Option<String>,
    store: Option<WireOrderStore>,
    store_name: Option<String>,
    store_id: Option<String>,
    delivery_address: Option<serde_json::Value>,
    collection_point: Option<WireCollectionPoint>,
    timeslot: Option<WireTimeslot>,
    service_fee: Option<i64>,
    bag_fee: Option<i64>,
}

#[derive(Deserialize)]
struct WireOrderStore {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct WireCollectionPoint {
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireTimeslot {
    date: Option<String>,
    slot: Option<String>,
}

#[derive(Deserialize)]
pub struct WireOrderDetail {
    summary: Option<WireOrderSummary>,
    products: Option<Vec<WireOrderProduct>>,
}

#[derive(Deserialize)]
pub struct WirePreviousPurchases {
    products: Option<Vec<WireOrderProduct>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireOrderProduct {
    product_id: Option<String>,
    name: Option<String>,
    brand: Option<String>,
    quantity: Option<f64>,
    /// A till receipt spells this `sale_type`; previous purchases `saleType`.
    #[serde(alias = "sale_type")]
    sale_type: Option<String>,
    price: Option<i64>,
}

impl WireOrderSummary {
    fn into_summary(self) -> OrderSummary {
        let store = self.store;
        OrderSummary {
            id: self.order_id.unwrap_or_default(),
            placed_at: self
                .order_timestamp
                .or_else(|| self.timeslot.as_ref().and_then(|t| t.date.clone())),
            total_cents: self
                .order_amount_in_cents
                .or(self.amount)
                .or(self.total_cost_in_cents),
            source: self.source.as_deref().and_then(Source::parse),
            store_name: self
                .store_name
                .or_else(|| store.as_ref().and_then(|s| s.name.clone())),
            store_id: self
                .store_id
                .or_else(|| store.as_ref().and_then(|s| s.id.clone())),
        }
    }
}

impl WireOrderPage {
    pub fn into_page(self) -> OrderPage {
        let orders: Vec<OrderSummary> = self
            .orders
            .unwrap_or_default()
            .into_iter()
            .map(WireOrderSummary::into_summary)
            .collect();
        let info = self.page_info;
        OrderPage {
            total: info
                .as_ref()
                .and_then(|i| i.total_contents_count)
                .unwrap_or(orders.len() as u32),
            total_pages: info.as_ref().and_then(|i| i.total_pages).unwrap_or(1),
            orders,
        }
    }
}

impl WireOrderProduct {
    fn into_line(self) -> OrderLine {
        let sku = self.product_id.unwrap_or_default();
        let sale_type = self
            .sale_type
            .as_deref()
            .and_then(SaleType::parse)
            .unwrap_or_else(|| SaleType::infer(&sku));
        OrderLine {
            sku,
            name: self.name.unwrap_or_default(),
            brand: self.brand.filter(|b| !b.trim().is_empty()),
            quantity: self.quantity.unwrap_or(0.0).max(0.0).round() as u32,
            sale_type,
            line_total_cents: self.price.unwrap_or(0),
        }
    }
}

fn into_lines(products: Option<Vec<WireOrderProduct>>) -> Vec<OrderLine> {
    products
        .unwrap_or_default()
        .into_iter()
        .map(WireOrderProduct::into_line)
        .collect()
}

impl WirePreviousPurchases {
    pub fn into_lines(self) -> Vec<OrderLine> {
        into_lines(self.products)
    }
}

impl WireOrderDetail {
    /// `None` when the response carries neither half, which is how the API says
    /// it has no such order.
    pub fn into_order(self) -> Option<Order> {
        if self.summary.is_none() && self.products.is_none() {
            return None;
        }
        let lines = into_lines(self.products);
        let Some(summary) = self.summary else {
            return Some(Order {
                summary: OrderSummary {
                    id: String::new(),
                    placed_at: None,
                    total_cents: None,
                    source: None,
                    store_name: None,
                    store_id: None,
                },
                lines,
                status: None,
                fulfilment: None,
                address: None,
                timeslot: None,
                service_fee_cents: 0,
                bag_fee_cents: 0,
            });
        };

        let status = summary.status.clone();
        let fulfilment = summary.fullfilment_method.clone();
        let address = summary
            .delivery_address
            .as_ref()
            .and_then(|a| a.as_str().map(str::to_string))
            .or_else(|| {
                summary
                    .collection_point
                    .as_ref()
                    .and_then(|c| c.name.clone())
            });
        let timeslot = summary.timeslot.as_ref().and_then(slot_label);
        let service_fee_cents = summary.service_fee.unwrap_or(0);
        let bag_fee_cents = summary.bag_fee.unwrap_or(0);

        Some(Order {
            summary: summary.into_summary(),
            lines,
            status,
            fulfilment,
            address,
            timeslot,
            service_fee_cents,
            bag_fee_cents,
        })
    }
}

fn slot_label(t: &WireTimeslot) -> Option<String> {
    match (t.date.as_deref(), t.slot.as_deref()) {
        (Some(d), Some(s)) => Some(format!("{d} {s}")),
        (Some(one), None) | (None, Some(one)) => Some(one.to_string()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_with_a_slash_is_a_till_receipt() {
        assert_eq!(
            Source::infer("region/fsni/banner/NW/customer/123"),
            Source::InStore
        );
        assert_eq!(Source::infer("abc123"), Source::Online);
    }

    #[test]
    fn accepts_the_spellings_people_type_for_a_source() {
        for s in ["in-store", "instore", "till", "shop"] {
            assert_eq!(Source::parse(s), Some(Source::InStore), "{s}");
        }
        for s in ["online", "web", "delivery"] {
            assert_eq!(Source::parse(s), Some(Source::Online), "{s}");
        }
        assert_eq!(Source::parse("yesterday"), None);
    }

    #[test]
    fn all_three_spellings_of_the_total_are_read() {
        for field in ["amount", "orderAmountInCents", "totalCostInCents"] {
            let raw = serde_json::json!({ "orders": [{ "orderId": "x", field: 4500 }] });
            let page = serde_json::from_value::<WireOrderPage>(raw)
                .unwrap()
                .into_page();
            assert_eq!(page.orders[0].total_cents, Some(4500), "{field}");
        }
    }

    #[test]
    fn the_apis_own_typo_is_accepted_as_spelled() {
        // "fullfilment", with the extra l, is how Foodstuffs spells it.
        let raw = serde_json::json!({
            "summary": { "orderId": "x", "fullfilmentMethod": "DELIVERY" },
            "products": []
        });
        let order = serde_json::from_value::<WireOrderDetail>(raw)
            .unwrap()
            .into_order()
            .unwrap();
        assert_eq!(order.fulfilment.as_deref(), Some("DELIVERY"));
    }

    #[test]
    fn both_spellings_of_a_lines_sale_type_are_read() {
        for field in ["sale_type", "saleType"] {
            let raw =
                serde_json::json!({ "products": [{ "productId": "A-EA-000", field: "WEIGHT" }] });
            let lines = serde_json::from_value::<WirePreviousPurchases>(raw)
                .unwrap()
                .into_lines();
            assert_eq!(lines[0].sale_type, SaleType::Weight, "{field}");
        }
    }

    #[test]
    fn a_page_with_no_page_info_still_reports_a_usable_total() {
        let raw = serde_json::json!({ "orders": [{ "orderId": "a" }, { "orderId": "b" }] });
        let page = serde_json::from_value::<WireOrderPage>(raw)
            .unwrap()
            .into_page();
        assert_eq!(page.total, 2);
        assert_eq!(page.total_pages, 1);
    }

    #[test]
    fn an_empty_response_is_how_the_api_says_no_such_order() {
        let raw = serde_json::json!({});
        assert!(serde_json::from_value::<WireOrderDetail>(raw)
            .unwrap()
            .into_order()
            .is_none());
    }

    #[test]
    fn a_till_receipt_carries_lines_without_a_summary() {
        let raw = serde_json::json!({ "products": [{ "productId": "A-EA-000", "price": 500 }] });
        let order = serde_json::from_value::<WireOrderDetail>(raw)
            .unwrap()
            .into_order()
            .unwrap();
        assert_eq!(order.lines.len(), 1);
        assert!(order.status.is_none());
    }

    #[test]
    fn a_collection_point_stands_in_for_a_delivery_address() {
        let raw = serde_json::json!({
            "summary": { "orderId": "x", "collectionPoint": { "name": "Thorndon pickup" } },
            "products": []
        });
        let order = serde_json::from_value::<WireOrderDetail>(raw)
            .unwrap()
            .into_order()
            .unwrap();
        assert_eq!(order.address.as_deref(), Some("Thorndon pickup"));
    }

    #[test]
    fn a_timeslot_joins_its_date_and_slot() {
        let raw = serde_json::json!({
            "summary": { "orderId": "x", "timeslot": { "date": "2026-09-04", "slot": "4-6pm" } },
            "products": []
        });
        let order = serde_json::from_value::<WireOrderDetail>(raw)
            .unwrap()
            .into_order()
            .unwrap();
        assert_eq!(order.timeslot.as_deref(), Some("2026-09-04 4-6pm"));
        assert_eq!(order.summary.placed_at.as_deref(), Some("2026-09-04"));
    }
}
