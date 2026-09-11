//! Per-store click-and-collect stock.
//!
//! The one part of these storefronts that is not GraphQL: a plain REST service
//! behind the same host, with a JSON-schema validator in front of it that is
//! stricter than anything else here. It answers for a whole basket of line
//! items at once, which is what makes `stock` on a cart one request rather than
//! one per line.
//!
//! **It answers for the basket, not for the line.** A request with three line
//! items comes back with one `pickupStatus`, and the worst line decides it:
//! asking about a jug that is in stock alongside a barcode that does not exist
//! answers `OOS` for both. So the two callers want different requests -- per
//! item for "can I collect this", the whole cart at once for "can I collect
//! this order" -- and [`crate::Client`] offers them separately rather than
//! pretending one answer can be split back up.
//!
//! Its schema is why [`Line`] carries six fields that look like they belong on
//! the product and not in a stock query. They are not optional and they are not
//! interchangeable:
//!
//! * `barcode` must match `^[a-zA-Z0-9_-]{1,20}$` -- an empty string is a
//!   validation error, not a wildcard. It is also what actually identifies the
//!   product: `productCode` is carried along but not checked, so a **configurable
//!   product has to be asked about by variant**, since only the variant has a
//!   barcode. Rebel Sport is mostly configurables.
//! * `saleAvailable` must be a **string**, while the product field it comes
//!   from (`saleavailability`) is an integer and its `_label` twin is usually
//!   `null`; [`crate::domain::Fulfilment::sale_available`] is what bridges that
//! * `selectedStoreId` is the store's **`store_id`**, not its
//!   `fulfilment_number` -- the opposite of `setStoreLocator`, which takes the
//!   fulfilment number. Sending the wrong one answers `NOT_FOUND_STORE` with
//!   `success: true`, which is the most misleading thing either site does.

use serde::Serialize;

use crate::domain::{Fulfilment, Stock};
use crate::wire::AvailabilityEnvelope;

/// The service's own name for "there is no such store", which it reports as a
/// successful call. Read explicitly so it cannot be mistaken for an answer.
pub const NOT_FOUND_STORE: &str = "NOT_FOUND_STORE";

/// One product in an availability request.
#[derive(Debug, Serialize)]
pub struct Line {
    #[serde(rename = "productCode")]
    pub product_code: String,
    pub barcode: String,
    #[serde(rename = "isDropship")]
    pub is_dropship: bool,
    #[serde(rename = "isClickCollectNotAvailable")]
    pub is_click_collect_not_available: bool,
    #[serde(rename = "productShippingBand")]
    pub product_shipping_band: String,
    #[serde(rename = "saleAvailable")]
    pub sale_available: String,
    pub category: String,
    #[serde(rename = "subCategory")]
    pub sub_category: String,
    pub quantity: u32,
}

impl Line {
    /// Build a line from a product's fulfilment fields.
    ///
    /// Returns `None` when the product carries no barcode: the validator
    /// rejects an empty one outright, so a request built without it fails for
    /// the whole basket rather than for the one line that lacked it.
    pub fn new(sku: &str, fulfilment: &Fulfilment, quantity: u32) -> Option<Line> {
        let barcode = fulfilment.barcode.clone()?;
        if barcode.is_empty() || barcode.len() > 20 {
            return None;
        }
        Some(Line {
            product_code: sku.to_string(),
            barcode,
            is_dropship: fulfilment.dropship,
            is_click_collect_not_available: fulfilment.click_collect_unavailable,
            product_shipping_band: fulfilment
                .shipping_band
                .clone()
                .unwrap_or_else(|| "Standard".into()),
            sale_available: fulfilment.sale_available().to_string(),
            category: fulfilment.category.clone().unwrap_or_default(),
            sub_category: fulfilment.subcategory.clone().unwrap_or_default(),
            quantity,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Request<'a> {
    #[serde(rename = "storeCode")]
    pub store_code: &'a str,
    /// The store's `store_id`, stringified. **Not** its `fulfilment_number`;
    /// see the module note.
    #[serde(rename = "selectedStoreId")]
    pub selected_store_id: String,
    #[serde(rename = "lineItems")]
    pub line_items: Vec<Line>,
}

/// Shape an answer.
///
/// `skus` is what was asked about, carried through so the one verdict is not
/// mistaken for a statement about a single product when a basket was sent.
pub fn stock(skus: Vec<String>, store_id: i64, body: &str) -> Result<Stock, serde_json::Error> {
    let envelope: AvailabilityEnvelope = serde_json::from_str(body)?;
    let data = envelope.data.as_ref();
    Stock {
        skus,
        store_id,
        pickup_status: data.and_then(|d| d.pickup_status.clone()),
        stock_level: data.and_then(|d| d.stock_level.clone()),
        message: data
            .and_then(|d| d.message.as_ref())
            .and_then(|m| m.value.clone())
            // With no store selected the service answers "Not found any store"
            // at the top level and no `data` at all; passing that through is
            // more useful than an empty row.
            .or_else(|| {
                (envelope.message_code.as_deref() == Some(NOT_FOUND_STORE))
                    .then(|| envelope.message.clone())
                    .flatten()
            }),
    }
    .pipe(Ok)
}

/// A tiny helper so the struct above reads as one expression.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;

    fn jug() -> Fulfilment {
        Fulfilment {
            barcode: Some("9414533081169".into()),
            category: Some("141".into()),
            subcategory: Some("14150".into()),
            shipping_band: Some("Standard".into()),
            dropship: false,
            click_collect_unavailable: false,
            sale_availability_label: None,
        }
    }

    #[test]
    fn a_product_with_no_barcode_cannot_be_asked_about() {
        // The validator rejects an empty barcode, and one bad line fails the
        // whole basket -- so this has to be caught before the request.
        let without = Fulfilment {
            barcode: None,
            ..jug()
        };
        assert!(Line::new("1103832", &without, 1).is_none());

        let too_long = Fulfilment {
            barcode: Some("0".repeat(21)),
            ..jug()
        };
        assert!(Line::new("1103832", &too_long, 1).is_none());
    }

    #[test]
    fn a_missing_availability_label_becomes_the_string_the_schema_demands() {
        // The product's own field is an integer and its label is null; the
        // service rejects anything but a string here.
        let line = Line::new("1103832", &jug(), 1).expect("a line with a barcode");
        let json = serde_json::to_value(&line).expect("serialises");
        assert_eq!(
            json["saleAvailable"].as_str(),
            Some(Fulfilment::DEFAULT_SALE_AVAILABILITY)
        );
        assert!(json["saleAvailable"].is_string());
    }

    #[test]
    fn the_request_names_the_fields_the_validator_wants() {
        let request = Request {
            store_code: "briscoes",
            // Albany's store_id. Its fulfilment_number is 1007, and sending
            // that instead answers NOT_FOUND_STORE -- as a success.
            selected_store_id: "291".into(),
            line_items: vec![Line::new("1103832", &jug(), 1).expect("a line")],
        };
        let json = serde_json::to_value(&request).expect("serialises");
        assert_eq!(json["storeCode"], "briscoes");
        assert_eq!(json["selectedStoreId"], "291");
        assert_eq!(json["lineItems"][0]["productCode"], "1103832");
        assert_eq!(json["lineItems"][0]["subCategory"], "14150");
        assert_eq!(json["lineItems"][0]["isDropship"], false);
    }

    #[test]
    fn an_answer_remembers_everything_it_was_asked_about() {
        // One verdict covers the whole basket, so a caller has to be able to
        // see that it asked about more than one thing.
        let body = r#"{"data":{"pickupStatus":"OOS","stockLevel":"out-stock"},
                       "messageCode":"SUCCESS","success":true}"#;
        let s = stock(vec!["8237741004".into(), "8237741005".into()], 444, body).expect("parses");
        assert_eq!(s.skus.len(), 2);
        assert_eq!(s.store_id, 444);
        assert_eq!(s.pickup_status.as_deref(), Some("OOS"));
    }

    #[test]
    fn an_answer_carries_the_status_and_the_phrase_the_site_would_show() {
        let body = r#"{"data":{"message":{"key":"next_day_timeframe","value":"Pick up tomorrow"},
                       "pickupStatus":"IN_STOCK_AFTER_CUTOFF","stockLevel":"in-stock"},
                       "message":"Success","messageCode":"SUCCESS","success":true}"#;
        let s = stock(vec!["1103832".into()], 291, body).expect("parses");
        assert_eq!(s.pickup_status.as_deref(), Some("IN_STOCK_AFTER_CUTOFF"));
        assert_eq!(s.stock_level.as_deref(), Some("in-stock"));
        assert_eq!(s.message.as_deref(), Some("Pick up tomorrow"));
    }

    #[test]
    fn an_unknown_store_is_reported_rather_than_read_as_an_empty_answer() {
        // The service calls this a success, which is the trap.
        let body =
            r#"{"message":"Not found any store","messageCode":"NOT_FOUND_STORE","success":true}"#;
        let s = stock(vec!["1103832".into()], 0, body).expect("parses");
        assert_eq!(s.pickup_status, None);
        assert_eq!(s.message.as_deref(), Some("Not found any store"));
    }
}
