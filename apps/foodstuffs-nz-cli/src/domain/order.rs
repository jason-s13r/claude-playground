//! Past orders.
//!
//! Two kinds, which Foodstuffs serves from two endpoints and this renders the
//! same way: orders placed online, and till receipts from shopping in a store,
//! which are linked to the account through Club Plus. Money is in cents
//! throughout, and a line's price is the line total, not the unit price.

use serde::{Deserialize, Serialize};

use crate::domain::cart::SaleType;
use crate::domain::product;

/// Where an order came from. `paged` filters on it, and it decides which
/// endpoint one order's detail is read from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    InStore,
    Online,
}

impl Source {
    /// What the API calls it, both in `?source=` and in `summary.source`.
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

    /// Accepts the spellings people type, and the one the API sends.
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

    /// In-store ids are paths -- "region/fsni/banner/NW/customer/..." -- and
    /// online ones a single opaque segment. That is enough to route an id
    /// somebody pasted to the right endpoint without listing the history first.
    pub fn infer(order_id: &str) -> Source {
        if order_id.contains('/') {
            Source::InStore
        } else {
            Source::Online
        }
    }
}

impl std::str::FromStr for Source {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Source> {
        Source::parse(s).ok_or_else(|| {
            anyhow::anyhow!("unknown source '{s}' (expected 'online' or 'in-store')")
        })
    }
}

/// One row of the history: enough to recognise an order, not what was in it.
#[derive(Clone, Debug, Serialize)]
pub struct OrderSummary {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placed_at: Option<String>,
    #[serde(rename = "total", serialize_with = "crate::domain::money::as_dollars")]
    pub total_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
}

impl OrderSummary {
    pub fn placed_label(&self) -> String {
        self.placed_at
            .as_deref()
            .map(short_datetime)
            .unwrap_or_else(|| "—".to_string())
    }

    pub fn source_label(&self) -> &'static str {
        self.source.map(Source::label).unwrap_or("—")
    }

    /// The source the API reported, or the one the id implies.
    pub fn resolved_source(&self) -> Source {
        self.source.unwrap_or_else(|| Source::infer(&self.id))
    }
}

/// One product on an order. Also what `previousPurchases` returns, which is the
/// same shape: the last time you bought a thing, and what it cost then.
#[derive(Clone, Debug, Serialize)]
pub struct OrderLine {
    pub sku: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    pub quantity: u32,
    pub sale_type: SaleType,
    #[serde(rename = "line_total")]
    pub line_total_cents: i64,
}

impl OrderLine {
    pub fn quantity_label(&self) -> String {
        self.sale_type.quantity_label(self.quantity)
    }

    pub fn title(&self) -> String {
        product::title(self.brand.as_deref(), &self.name)
    }
}

/// One order and what was in it.
#[derive(Clone, Debug, Serialize)]
pub struct Order {
    #[serde(flatten)]
    pub summary: OrderSummary,
    pub lines: Vec<OrderLine>,
    /// The fields below only ever arrive on an online order; a till receipt
    /// carries none of them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfilment: Option<String>,
    /// Delivery address, or the name of the pickup point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeslot: Option<String>,
    pub service_fee_cents: i64,
    pub bag_fee_cents: i64,
}

impl Order {
    pub fn lines_total_cents(&self) -> i64 {
        self.lines.iter().map(|l| l.line_total_cents).sum()
    }

    pub fn summary_line(&self) -> String {
        format!(
            "{} line{}, {}",
            self.lines.len(),
            crate::output::plural(self.lines.len()),
            crate::domain::dollars(
                self.summary
                    .total_cents
                    .unwrap_or_else(|| self.lines_total_cents())
            )
        )
    }
}

/// One page of history, plus how much more there is.
pub struct OrderPage {
    pub orders: Vec<OrderSummary>,
    pub total: u32,
    pub total_pages: u32,
}

/// "2026-08-01T16:00:00+12:00" becomes "2026-08-01 16:00".
///
/// No date library: the API sends the store's own local time, so there is
/// nothing to convert, only a string to cut down to what a person reads.
pub fn short_datetime(iso: &str) -> String {
    let mut parts = iso.splitn(2, 'T');
    let date = parts.next().unwrap_or_default();
    match parts.next().and_then(|time| time.get(..5)) {
        Some(hhmm) => format!("{date} {hhmm}"),
        None => date.to_string(),
    }
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

/// The summary, which the list and the detail endpoints both return. Online
/// orders carry more of it than till receipts do.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireOrderSummary {
    order_id: Option<String>,
    /// A till receipt calls the total `amount`; an online order spells the same
    /// number one of the longer ways.
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
        (Some(date), Some(slot)) => Some(format!("{} {slot}", date.split('T').next()?)),
        (Some(date), None) => Some(short_datetime(date)),
        (None, Some(slot)) => Some(slot.to_string()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One row of the list endpoint, as the API sends it.
    fn wire_row() -> serde_json::Value {
        serde_json::json!({
            "orderId": "region/fsni/banner/NW/customer/42/salesstaginglink/_S_000001234_D_20260801",
            "amount": 1486,
            "orderTimestamp": "2026-08-01T16:00:00+12:00",
            "store": { "name": "New World Thorndon", "id": "store-1", "region": "NI" },
            "source": "IN_STORE"
        })
    }

    #[test]
    fn timestamps_are_cut_down_to_what_a_person_reads() {
        assert_eq!(
            short_datetime("2026-08-01T16:00:00+12:00"),
            "2026-08-01 16:00"
        );
        // A date on its own, and a value in no shape at all, both survive.
        assert_eq!(short_datetime("2026-08-01"), "2026-08-01");
        assert_eq!(short_datetime("T1"), "");
    }

    #[test]
    fn the_id_says_which_endpoint_an_order_lives_at() {
        assert_eq!(
            Source::infer("region/fsni/banner/NW/customer/42"),
            Source::InStore
        );
        assert_eq!(Source::infer("f81d4fae-7dec-11d0"), Source::Online);
    }

    #[test]
    fn the_source_flag_accepts_what_people_type() {
        for s in ["in-store", "InStore", "IN_STORE", "store"] {
            assert_eq!(Source::parse(s), Some(Source::InStore), "{s}");
        }
        for s in ["online", "ONLINE", "web"] {
            assert_eq!(Source::parse(s), Some(Source::Online), "{s}");
        }
        assert_eq!(Source::parse("click and collect"), None);
    }

    #[test]
    fn a_page_carries_how_much_more_history_there_is() {
        let page: WireOrderPage = serde_json::from_value(serde_json::json!({
            "pageInfo": { "pageNumber": 1, "totalPages": 2, "totalContentsCount": 16 },
            "orders": [wire_row()],
        }))
        .unwrap();
        let page = page.into_page();

        assert_eq!(page.total, 16);
        assert_eq!(page.total_pages, 2);
        let order = &page.orders[0];
        assert_eq!(order.total_cents, Some(1486));
        assert_eq!(order.source, Some(Source::InStore));
        assert_eq!(order.store_name.as_deref(), Some("New World Thorndon"));
        assert_eq!(order.placed_label(), "2026-08-01 16:00");
    }

    #[test]
    fn a_till_receipt_itemises_to_its_total() {
        let detail: WireOrderDetail = serde_json::from_value(serde_json::json!({
            "summary": wire_row(),
            "products": [
                {
                    "productId": "5011234-EA-000", "quantity": 2, "sale_type": "UNITS",
                    "price": 774, "name": "Creamy Milk Chocolate Block", "brand": "Whittaker's"
                },
                {
                    "productId": "5101234-KGM-000", "quantity": 1000, "sale_type": "WEIGHT",
                    "price": 712, "name": "Whole Almonds"
                }
            ]
        }))
        .unwrap();
        let order = detail.into_order().expect("an order with both halves");

        assert_eq!(order.lines.len(), 2);
        // The brand is carried apart from the name and joined for display.
        assert_eq!(
            order.lines[0].title(),
            "Whittaker's Creamy Milk Chocolate Block"
        );
        assert_eq!(order.lines[0].quantity_label(), "2");
        assert_eq!(order.lines[1].quantity_label(), "1kg");
        assert_eq!(order.lines_total_cents(), 1486);
        assert_eq!(order.summary.total_cents, Some(1486));
        // Nothing online about a till receipt.
        assert_eq!(order.timeslot, None);
        assert_eq!(order.bag_fee_cents, 0);
    }

    #[test]
    fn an_online_order_carries_its_slot_and_its_fees() {
        let detail: WireOrderDetail = serde_json::from_value(serde_json::json!({
            "summary": {
                "orderId": "9f1c",
                "orderAmountInCents": 4200,
                "status": "DELIVERED",
                "fullfilmentMethod": "DELIVERY",
                "storeName": "New World Thorndon",
                "deliveryAddress": "1 Molesworth St, Wellington",
                "timeslot": { "date": "2026-08-01T00:00:00+12:00", "slot": "10:00 - 12:00" },
                "serviceFee": 500,
                "bagFee": 150,
                "source": "ONLINE"
            },
            "products": [{
                "productId": "5039956-EA-000", "quantity": 1, "saleType": "UNITS",
                "price": 3550, "name": "Broccoli"
            }]
        }))
        .unwrap();
        let order = detail.into_order().unwrap();

        assert_eq!(order.status.as_deref(), Some("DELIVERED"));
        assert_eq!(order.fulfilment.as_deref(), Some("DELIVERY"));
        assert_eq!(order.timeslot.as_deref(), Some("2026-08-01 10:00 - 12:00"));
        assert_eq!(
            order.address.as_deref(),
            Some("1 Molesworth St, Wellington")
        );
        assert_eq!(order.service_fee_cents, 500);
        assert_eq!(order.summary.total_cents, Some(4200));
        // `saleType` here, `sale_type` on a till receipt; both have to parse.
        assert_eq!(order.lines[0].sale_type, SaleType::Units);
        // Fees are why the lines do not add up to the total.
        assert_eq!(order.lines_total_cents(), 3550);
    }

    #[test]
    fn an_order_that_is_not_there_is_not_an_empty_one() {
        let detail: WireOrderDetail = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(detail.into_order().is_none());
    }

    #[test]
    fn previous_purchases_are_ordinary_lines() {
        let wire: WirePreviousPurchases = serde_json::from_value(serde_json::json!({
            "products": [{
                "productId": "5101234-KGM-000", "name": "Whole Almonds",
                "quantity": 1000, "price": 4000, "saleType": "WEIGHT", "isCatered": false
            }]
        }))
        .unwrap();
        let lines = wire.into_lines();

        assert_eq!(lines[0].sku, "5101234-KGM-000");
        assert_eq!(lines[0].quantity_label(), "1kg");
        assert_eq!(lines[0].line_total_cents, 4000);
    }

    #[test]
    fn a_renamed_field_narrows_the_row_rather_than_failing_it() {
        let page: WireOrderPage =
            serde_json::from_value(serde_json::json!({ "orders": [{ "orderId": "9f1c" }] }))
                .unwrap();
        let page = page.into_page();
        let order = &page.orders[0];

        assert_eq!(order.total_cents, None);
        assert_eq!(order.placed_label(), "—");
        assert_eq!(order.source_label(), "—");
        // With no source reported, the id still says where to read it from.
        assert_eq!(order.resolved_source(), Source::Online);
        // Falls back to counting what came back.
        assert_eq!(page.total, 1);
    }
}
