//! What this crate answers with.
//!
//! Vendor-shaped, not shared: these are Briscoe Group's ideas, and there is no
//! grocery domain crate to conform to. Nearly everything is optional, because
//! both halves of the data are undocumented -- Magento's custom attributes and
//! Klevu's index -- and a field either side renames should cost a column rather
//! than the whole command.

use serde::{Deserialize, Serialize};

use crate::banner::Banner;

/// What the storefront publishes about itself.
///
/// Read at run time rather than compiled in. Every value here is public, and
/// every one of them is per-fascia, so a hardcoded copy would be two copies
/// that go stale independently.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Storefront {
    pub store_code: String,
    /// Klevu's host for this fascia, bare: `aucs34.ksearchnet.com`.
    pub klevu_url: Option<String>,
    pub klevu_key: Option<String>,
}

/// Which SAP Customer Data Cloud site a fascia authenticates against.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GigyaConfig {
    pub enabled: bool,
    pub api_key: String,
    pub data_center: String,
    /// The screen set that shows the sign-in form.
    ///
    /// Only a browser needs these, and only because Gigya loads its captcha
    /// when the sign-in screen opens rather than on page load -- so something
    /// has to be able to open it. Passed through rather than hardcoded: they
    /// are per-fascia (`Briscoes-RegistrationLogin`, and Rebel Sport's own).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_screen_set: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_start_screen: Option<String>,
}

/// Money, as the storefront sends it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Money {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

impl Money {
    pub fn new(value: f64, currency: Option<String>) -> Money {
        Money { value, currency }
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${:.2}", self.value)
    }
}

/// A product as a listing knows it.
///
/// Comes out of Klevu, whose every numeric field is a string on the wire, so
/// the parsing happens once in [`crate::wire`] and callers see numbers.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Product {
    pub sku: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    /// What it cost before the current promotion, when there is one. Klevu
    /// sends `price` and `salePrice` as the same number when nothing is on
    /// special, so this is `None` rather than a fake strikethrough.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub was_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

impl Product {
    /// The saving, when the product is actually marked down.
    pub fn discount(&self) -> Option<f64> {
        let (price, was) = (self.price?, self.was_price?);
        (was > price).then_some(was - price)
    }
}

/// A page of products, plus the facets the index offered for narrowing it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Listing {
    pub banner: Banner,
    pub products: Vec<Product>,
    pub total: u64,
    pub offset: u64,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub facets: Vec<Facet>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Facet {
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub options: Vec<FacetOption>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FacetOption {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

/// A product as its own page knows it.
///
/// The four fields between `barcode` and `shipping_band` look like noise and
/// are not: the click-and-collect service refuses a request without them, so
/// they are why [`crate::Client::stock`] takes one of these rather than a SKU.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductDetail {
    pub sku: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub was_price: Option<Money>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub images: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attributes: Vec<Attribute>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub categories: Vec<String>,
    /// Everything the availability API needs, carried so a caller never has to
    /// fetch the product twice.
    pub fulfilment: Fulfilment,
    /// The sizes and colours, where the product has them.
    ///
    /// Not decoration: a configurable product has **no barcode of its own** --
    /// only its variants do -- and the stock service identifies a product by
    /// barcode. So on Rebel Sport, where most of the catalogue is configurable,
    /// asking about stock means asking about a variant. [`ProductDetail::stockable`]
    /// is what picks the right one.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub variants: Vec<Variant>,
}

/// One size-and-colour of a configurable product.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Variant {
    pub sku: String,
    /// The choices that name it: `sizecode: US10`, `colourcode: White/Red`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub options: Vec<Attribute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock: Option<bool>,
    pub fulfilment: Fulfilment,
}

impl Variant {
    /// `US10 / White/Red`, for a column.
    pub fn label(&self) -> String {
        self.options
            .iter()
            .map(|o| o.value.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

impl ProductDetail {
    /// What can actually be asked about at a till, and under which barcode.
    ///
    /// A simple product is itself. A configurable one is its variants, because
    /// the parent has no barcode and the stock service will not answer without
    /// one. Returns empty when nothing in the product has a barcode at all,
    /// which is the honest answer to "where can I collect this".
    pub fn stockable(&self) -> Vec<(&str, &Fulfilment)> {
        if self.fulfilment.barcode.is_some() {
            return vec![(self.sku.as_str(), &self.fulfilment)];
        }
        self.variants
            .iter()
            .filter(|v| v.fulfilment.barcode.is_some())
            .map(|v| (v.sku.as_str(), &v.fulfilment))
            .collect()
    }

    /// One variant by SKU, for a caller that already knows which size it wants.
    pub fn variant(&self, sku: &str) -> Option<&Variant> {
        self.variants.iter().find(|v| v.sku == sku)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

/// The fields the click-and-collect service validates against.
///
/// Every one of them arrives loosely typed. `isdropship` is the integer `0`,
/// not `false`; `saleavailability` is the integer `20127` while the `_label`
/// beside it is often `null`; and the service's own schema demands
/// `saleAvailable` be a *string*. Hence [`Fulfilment::sale_available`], which
/// supplies the phrase the API expects when the product does not carry one.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Fulfilment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcategory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shipping_band: Option<String>,
    pub dropship: bool,
    pub click_collect_unavailable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sale_availability_label: Option<String>,
}

impl Fulfilment {
    /// The site's default phrase, used when a product carries no label of its
    /// own -- which is most of them. The availability service validates the
    /// *type* of this field and not its contents, so a default is honest here
    /// in a way it would not be if the value were being interpreted.
    pub const DEFAULT_SALE_AVAILABILITY: &'static str = "Online and In store Fulfilled";

    pub fn sale_available(&self) -> &str {
        self.sale_availability_label
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(Self::DEFAULT_SALE_AVAILABILITY)
    }
}

/// A shop.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Store {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// The id the availability API and `setStoreLocator` both take. Not the
    /// same number as `id`, and mixing them up answers `NOT_FOUND_STORE`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfilment_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postcode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_and_collect: Option<bool>,
    /// The same-day collection cutoff, as a local clock time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_day_cutoff: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub hours: Vec<OpeningHours>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpeningHours {
    pub day: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<String>,
}

/// What the click-and-collect service said about a basket at a store.
///
/// One verdict for everything asked about, because that is what the service
/// answers: send two line items and the worse of them decides the whole
/// result. `skus` records what was covered so a caller cannot mistake a basket
/// answer for a statement about one product.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stock {
    pub skus: Vec<String>,
    pub store_id: i64,
    /// `IN_STOCK`, `IN_STOCK_AFTER_CUTOFF`, `OUT_OF_STOCK` and friends. Passed
    /// through rather than mapped to a boolean: "in stock but after today's
    /// cutoff" is a real third answer and flattening it loses the useful half.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_level: Option<String>,
    /// The phrase the site would show: "Pick up tomorrow", "Pick up in 2-4 days".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A category, as the flat tree lists it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
    /// Base64 of the numeric id, which is what Magento takes.
    pub uid: String,
    /// The numeric id, which is what Klevu takes.
    pub id: String,
    pub name: String,
    pub level: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_path: Option<String>,
    pub children: u64,
}

/// The cart.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cart {
    pub id: String,
    pub total_quantity: f64,
    pub lines: Vec<CartLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtotal: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<Money>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub discounts: Vec<Discount>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub coupons: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CartLine {
    /// The line's own id, which is what an update or a removal takes -- not the
    /// SKU, and not the product uid.
    pub uid: String,
    pub sku: String,
    pub name: String,
    pub quantity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_total: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<String>,
    pub fulfilment: Fulfilment,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Discount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub amount: Money,
}

/// The saved-items list.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wishlist {
    pub id: String,
    pub count: u64,
    pub items: Vec<WishlistItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WishlistItem {
    /// The item's id in the list, which is what a removal takes.
    pub id: String,
    pub sku: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock: Option<bool>,
}

/// Who is signed in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Customer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    /// `ZW`, `L1` and so on -- the SAP customer type, which is what decides
    /// whether member pricing shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_code: Option<String>,
    /// The fulfilment number of the store filed against the account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
}

/// An order placed online.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Order {
    pub number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shipping: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_store: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<Money>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderPage {
    pub orders: Vec<Order>,
    pub page: u64,
    pub pages: u64,
}

/// One online order in full, with its shipments.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderDetail {
    pub number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub shipments: Vec<Shipment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub payment_methods: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub invoices: Vec<Invoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtotal: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shipping: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loyalty_reward: Option<Money>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub discounts: Vec<Discount>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shipment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The carrier's own tracking page, ready to open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracking: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub lines: Vec<OrderLine>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderLine {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    pub quantity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<Money>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invoice {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
}

/// A till receipt from a shop.
///
/// Not an [`Order`] with a flag: these come out of SAP rather than Magento,
/// they are keyed by a receipt reference rather than an order number, and they
/// carry the store you stood in. See [`crate::gql::IN_STORE_ORDERS`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    pub lines: Vec<ReceiptLine>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceiptLine {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub quantity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
}

/// A page of in-store receipts, and what the service said about the window.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceiptPage {
    pub receipts: Vec<Receipt>,
    /// The window the service actually searched, which it decides rather than
    /// the caller when no dates were given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// True when the service says there is older data behind this window.
    pub more_history: bool,
}

/// Progress toward the next loyalty reward.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Loyalty {
    pub member: bool,
    pub eligible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_spend: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_next_reward: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub vouchers: Vec<Voucher>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voucher {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
    pub applied: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_discount_is_only_reported_when_the_price_actually_fell() {
        // Klevu sends price and salePrice as the same number for everything
        // that is not on special, so a naive subtraction would put a $0.00
        // saving on the whole catalogue.
        let flat = Product {
            sku: "1103832".into(),
            name: "Prestige Lotus Filter Jug".into(),
            brand: None,
            price: Some(79.99),
            was_price: Some(79.99),
            in_stock: Some(true),
            url: None,
            image: None,
        };
        assert_eq!(flat.discount(), None);

        let marked_down = Product {
            price: Some(59.99),
            ..flat.clone()
        };
        let saving = marked_down.discount().expect("a marked-down product saves");
        assert!((saving - 20.0).abs() < 0.005, "{saving}");
    }

    #[test]
    fn a_product_with_no_availability_label_still_has_one_to_send() {
        // The availability service rejects a null here with a type error, and
        // most products carry nothing.
        let empty = Fulfilment::default();
        assert_eq!(
            empty.sale_available(),
            Fulfilment::DEFAULT_SALE_AVAILABILITY
        );

        let blank = Fulfilment {
            sale_availability_label: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(
            blank.sale_available(),
            Fulfilment::DEFAULT_SALE_AVAILABILITY,
            "whitespace is not a label"
        );

        let real = Fulfilment {
            sale_availability_label: Some("Online Only".into()),
            ..Default::default()
        };
        assert_eq!(real.sale_available(), "Online Only");
    }
}
