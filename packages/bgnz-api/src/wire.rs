//! What the wire actually carries.
//!
//! Three undocumented dialects -- Magento's custom attributes, Klevu's index
//! and Gigya's REST -- so the rule here is that **nothing is required**. Every
//! field is `Option` or defaults, because a field any of the three renames
//! should cost a column rather than the whole command.
//!
//! The loose types are not hypothetical. In one answer from one product query:
//! `isdropship` is the integer `0` where a boolean belongs, `saleavailability`
//! is the integer `20127` while the `_label` beside it is `null`, and every
//! price Klevu returns is a quoted string. A store's `working_time` goes
//! further and arrives as a JSON *document inside a JSON string*.

use serde::{Deserialize, Deserializer};

use crate::domain::{
    Attribute, Cart, CartLine, Category, Customer, Discount, Fulfilment, Invoice, Loyalty, Money,
    OpeningHours, Order, OrderDetail, OrderLine, Product, Receipt, ReceiptLine, Shipment, Store,
    Variant, Voucher, Wishlist, WishlistItem,
};

// ------------------------------------------------------------ loose scalars

/// A number that is sometimes quoted, and sometimes absent.
///
/// Klevu prices are strings (`"79.99"`); Magento's are numbers. A coordinate or
/// a price that will not parse should cost a column, not the record.
pub fn loose_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    })
}

/// A boolean that is sometimes an integer and sometimes a word.
///
/// Magento sends `isdropship: 0`, Klevu sends `inStock: "yes"`, and Gigya
/// sends `isActive: "True"`. All three mean a boolean.
pub fn loose_bool<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::Bool(b)) => Some(b),
        Some(serde_json::Value::Number(n)) => n.as_i64().map(|n| n != 0),
        Some(serde_json::Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "yes" | "true" | "1" | "y" => Some(true),
            "no" | "false" | "0" | "n" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

/// A list that is sometimes `null`.
///
/// `#[serde(default)]` covers an *absent* field and nothing else, so a
/// storefront that sends `applied_coupons: null` for an empty cart fails the
/// whole decode with "invalid type: null, expected a sequence". Magento does
/// this for most of its optional collections, so every list on the wire goes
/// through here.
pub fn nullable_vec<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

/// A string that is sometimes a number.
///
/// `sapcategory` is `"141"` on one product and `141` on the next, and it has to
/// reach the availability API as a string either way.
pub fn loose_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => Some(s),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

fn yes(v: Option<bool>) -> bool {
    v.unwrap_or(false)
}

// ------------------------------------------------------------------- GraphQL

#[derive(Debug, Deserialize)]
pub struct GqlEnvelope<T> {
    pub data: Option<T>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub errors: Vec<GqlError>,
}

#[derive(Debug, Deserialize)]
pub struct GqlError {
    pub message: Option<String>,
    #[serde(default)]
    pub extensions: Option<GqlExtensions>,
}

#[derive(Debug, Deserialize)]
pub struct GqlExtensions {
    pub category: Option<String>,
}

impl GqlError {
    /// Magento's own classification, `graphql-authorization` and the like. The
    /// message is free text and changes; this does not.
    pub fn category(&self) -> Option<&str> {
        self.extensions.as_ref()?.category.as_deref()
    }
}

// ---------------------------------------------------------------- storeConfig

#[derive(Debug, Deserialize)]
pub struct StoreConfigEnvelope {
    #[serde(rename = "storeConfig")]
    pub store_config: Option<StoreConfig>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StoreConfig {
    pub store_code: Option<String>,
    pub klevu_search_url: Option<String>,
    pub klevu_search_js_api_key: Option<String>,
    #[serde(default, deserialize_with = "loose_bool")]
    pub gigya_enable: Option<bool>,
    pub gigya_api_key: Option<String>,
    pub gigya_data_center: Option<String>,
    pub gigya_login_screen_set: Option<String>,
    pub gigya_login_start_screen: Option<String>,
    pub customer_token_timeout: Option<i64>,
    pub loyalty_my_account_description: Option<String>,
}

// -------------------------------------------------------------------- product

#[derive(Debug, Deserialize)]
pub struct ProductsEnvelope {
    pub products: Option<ProductsResult>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProductsResult {
    #[serde(default)]
    pub total_count: u64,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub items: Vec<ProductNode>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProductNode {
    pub sku: Option<String>,
    pub name: Option<String>,
    pub url_key: Option<String>,
    pub stock_status: Option<String>,
    pub brand: Option<String>,
    pub product_description: Option<String>,
    pub image: Option<UrlNode>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub media_gallery: Vec<UrlNode>,
    #[serde(default, deserialize_with = "loose_string")]
    pub barcode: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub sapcategory: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub subcategory: Option<String>,
    pub shipping_band: Option<String>,
    #[serde(default, deserialize_with = "loose_bool")]
    pub isdropship: Option<bool>,
    #[serde(default, deserialize_with = "loose_bool")]
    pub isclickcollectnotavailable: Option<bool>,
    #[serde(default, deserialize_with = "loose_bool")]
    pub productisclickcollectnotavailable: Option<bool>,
    pub saleavailability_label: Option<String>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub categories: Vec<CategoryNode>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub variants: Vec<VariantNode>,
    pub price_range: Option<PriceRange>,
}

#[derive(Debug, Deserialize)]
pub struct VariantNode {
    #[serde(default, deserialize_with = "nullable_vec")]
    pub attributes: Vec<VariantAttribute>,
    pub product: Option<ProductNode>,
}

#[derive(Debug, Deserialize)]
pub struct VariantAttribute {
    pub code: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub label: Option<String>,
}

impl VariantNode {
    pub fn variant(&self) -> Option<Variant> {
        let product = self.product.as_ref()?;
        Some(Variant {
            sku: product.sku.clone()?,
            options: self
                .attributes
                .iter()
                .filter_map(|a| {
                    Some(Attribute {
                        name: a.code.clone()?,
                        value: a.label.clone()?,
                    })
                })
                .collect(),
            in_stock: product.in_stock(),
            fulfilment: product.fulfilment(),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct UrlNode {
    pub url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CustomAttributes {
    #[serde(default, deserialize_with = "nullable_vec")]
    pub items: Vec<CustomAttribute>,
}

#[derive(Debug, Deserialize)]
pub struct CustomAttribute {
    pub code: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub value: Option<String>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub selected_options: Vec<SelectedOption>,
}

#[derive(Debug, Deserialize)]
pub struct SelectedOption {
    pub label: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub value: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PriceRange {
    pub minimum_price: Option<PricePoint>,
    pub maximum_price: Option<PricePoint>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PricePoint {
    pub final_price: Option<MoneyNode>,
    pub regular_price: Option<MoneyNode>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MoneyNode {
    #[serde(default, deserialize_with = "loose_f64")]
    pub value: Option<f64>,
    pub currency: Option<String>,
}

impl MoneyNode {
    pub fn money(&self) -> Option<Money> {
        Some(Money::new(self.value?, self.currency.clone()))
    }
}

#[derive(Debug, Deserialize)]
pub struct CategoryNode {
    pub uid: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub level: i64,
    pub url_path: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub children_count: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CategoriesEnvelope {
    pub categories: Option<CategoriesResult>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CategoriesResult {
    #[serde(default)]
    pub total_count: u64,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub items: Vec<CategoryNode>,
}

impl ProductNode {
    pub fn fulfilment(&self) -> Fulfilment {
        Fulfilment {
            barcode: self.barcode.clone(),
            category: self.sapcategory.clone(),
            subcategory: self.subcategory.clone(),
            shipping_band: self.shipping_band.clone(),
            dropship: yes(self.isdropship),
            // Two spellings of the same idea, and a product only has to be
            // barred by one of them.
            click_collect_unavailable: yes(self.isclickcollectnotavailable)
                || yes(self.productisclickcollectnotavailable),
            sale_availability_label: self.saleavailability_label.clone(),
        }
    }

    pub fn in_stock(&self) -> Option<bool> {
        match self.stock_status.as_deref()? {
            "IN_STOCK" => Some(true),
            "OUT_OF_STOCK" => Some(false),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SpecsEnvelope {
    pub product_specs: Option<ProductSpecs>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProductSpecs {
    #[serde(default, deserialize_with = "nullable_vec")]
    pub key_specs: Vec<Spec>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub general_specs: Vec<Spec>,
}

#[derive(Debug, Deserialize)]
pub struct Spec {
    pub spec_text: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub spec_value: Option<String>,
}

impl ProductSpecs {
    /// Key specs first, then the general ones, which is the order the product
    /// page puts them in.
    pub fn attributes(&self) -> Vec<Attribute> {
        self.key_specs
            .iter()
            .chain(self.general_specs.iter())
            .filter_map(|s| {
                let value = strip_tags(s.spec_value.as_deref()?);
                (!value.is_empty()).then_some(Attribute {
                    name: strip_tags(s.spec_text.as_deref()?),
                    value,
                })
            })
            .collect()
    }
}

/// Plain text out of a spec value.
///
/// The spec sheet is merchandising copy pasted into a field, so a value can
/// arrive as `</p>Replace the filter after every 35 days</p>` -- unbalanced
/// tags included. This is a table cell, not a document: the markup is noise
/// either way, and a parser would be a dependency for the sake of an artefact.
pub fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for c in text.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    // `&nbsp;` and friends turn up in the same copy.
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Base64 of a Magento uid, decoded to the numeric id Klevu indexes by.
///
/// Hand-rolled rather than pulling in `base64`: the alphabet here is always the
/// standard one and the payload is always a short run of digits, so the whole
/// job is four characters to three bytes with the padding trimmed.
pub fn decode_uid(uid: &str) -> Option<String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut nbits = 0u32;
    let mut out = Vec::new();
    for c in uid.bytes() {
        if c == b'=' {
            break;
        }
        let v = TABLE.iter().position(|&t| t == c)? as u32;
        bits = (bits << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    let text = String::from_utf8(out).ok()?;
    // Only a digit run is a category id. Anything else is a uid for something
    // this is not meant to decode, and guessing would be worse than declining.
    text.chars()
        .all(|c| c.is_ascii_digit())
        .then_some(())
        .filter(|_| !text.is_empty())?;
    Some(text)
}

impl CategoryNode {
    pub fn category(&self) -> Option<Category> {
        let uid = self.uid.clone()?;
        Some(Category {
            id: decode_uid(&uid).unwrap_or_else(|| uid.clone()),
            uid,
            name: self.name.clone().unwrap_or_default(),
            level: self.level,
            url_path: self.url_path.clone(),
            children: self
                .children_count
                .as_deref()
                .and_then(|c| c.parse().ok())
                .unwrap_or(0),
        })
    }
}

// --------------------------------------------------------------------- Klevu

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlevuEnvelope {
    #[serde(default, deserialize_with = "nullable_vec")]
    pub query_results: Vec<KlevuQueryResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlevuQueryResult {
    /// The id this block was asked under. A page asks several at once, so the
    /// answer has to be matched back rather than taken positionally.
    pub id: Option<String>,
    #[serde(default)]
    pub meta: KlevuMeta,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub records: Vec<KlevuRecord>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub filters: Vec<KlevuFilter>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlevuMeta {
    #[serde(default)]
    pub total_results_found: u64,
    #[serde(default)]
    pub offset: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlevuRecord {
    pub sku: Option<String>,
    pub name: Option<String>,
    pub brand: Option<String>,
    /// What it sells for now.
    #[serde(default, deserialize_with = "loose_f64")]
    pub sale_price: Option<f64>,
    /// The ticket price, equal to `sale_price` when nothing is on special.
    #[serde(default, deserialize_with = "loose_f64")]
    pub price: Option<f64>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub base_price: Option<f64>,
    #[serde(default, deserialize_with = "loose_bool")]
    pub in_stock: Option<bool>,
    pub url: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlevuFilter {
    pub key: Option<String>,
    pub label: Option<String>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub options: Vec<KlevuFilterOption>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlevuFilterOption {
    pub name: Option<String>,
    pub value: Option<String>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub count: Option<f64>,
}

impl KlevuRecord {
    pub fn product(&self) -> Option<Product> {
        let sku = self.sku.clone()?;
        // `salePrice` is what it costs; `price` is the ticket. When they agree
        // there is no promotion, and claiming one would put a $0.00 saving on
        // the whole catalogue.
        let now = self.sale_price.or(self.price);
        let ticket = self.price.or(self.base_price);
        let was = ticket.filter(|t| now.is_some_and(|n| *t > n));
        Some(Product {
            sku,
            name: self.name.clone().unwrap_or_default(),
            brand: self.brand.clone(),
            price: now,
            was_price: was,
            in_stock: self.in_stock,
            url: self.url.clone(),
            image: self.image.clone(),
        })
    }
}

// -------------------------------------------------------------------- stores

#[derive(Debug, Deserialize)]
pub struct RegionsEnvelope {
    #[serde(rename = "getRegion", default, deserialize_with = "nullable_vec")]
    pub regions: Vec<RegionNode>,
}

#[derive(Debug, Deserialize)]
pub struct RegionNode {
    pub region_name: Option<String>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub store_items: Vec<StoreNode>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StoreNode {
    #[serde(default)]
    pub store_id: i64,
    pub store_locator_name: Option<String>,
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub fulfilment_number: Option<String>,
    pub line1: Option<String>,
    pub line2: Option<String>,
    pub city: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub postcode: Option<String>,
    pub phone: Option<String>,
    /// A JSON array, inside a JSON string. See [`opening_hours`].
    pub working_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClickAndCollectEnvelope {
    #[serde(rename = "getStoreLocator", default, deserialize_with = "nullable_vec")]
    pub stores: Vec<ClickAndCollectNode>,
}

#[derive(Debug, Deserialize)]
pub struct ClickAndCollectNode {
    #[serde(default)]
    pub store_id: i64,
    #[serde(default, deserialize_with = "loose_string")]
    pub fulfilment_number: Option<String>,
    #[serde(default, deserialize_with = "loose_bool")]
    pub is_click_and_collect: Option<bool>,
    pub same_day_delivery: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkingTime {
    day_of_week: Option<String>,
    open_time: Option<String>,
    close_time: Option<String>,
}

/// Opening hours, out of the JSON document the storefront nests inside a string.
///
/// Escaped twice on the wire, which is why this parses rather than deserialises
/// in place: the outer value is a `String` as far as the schema is concerned,
/// and a malformed inner document should cost the hours rather than the store.
pub fn opening_hours(raw: Option<&str>) -> Vec<OpeningHours> {
    let Some(raw) = raw else { return Vec::new() };
    let Ok(days) = serde_json::from_str::<Vec<WorkingTime>>(raw) else {
        return Vec::new();
    };
    days.into_iter()
        .filter_map(|d| {
            Some(OpeningHours {
                day: d.day_of_week?,
                open: d.open_time.filter(|t| !t.is_empty()),
                close: d.close_time.filter(|t| !t.is_empty()),
            })
        })
        .collect()
}

impl StoreNode {
    pub fn store(&self, region: Option<&str>) -> Store {
        let address = [self.line1.as_deref(), self.line2.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        Store {
            id: self.store_id,
            name: self.store_locator_name.clone().unwrap_or_default(),
            display_name: self.display_name.clone(),
            fulfilment_number: self.fulfilment_number.clone(),
            region: region.map(str::to_string),
            address: (!address.is_empty()).then_some(address),
            city: self.city.clone(),
            postcode: self.postcode.clone(),
            phone: self.phone.as_deref().map(|p| p.trim().to_string()),
            click_and_collect: None,
            same_day_cutoff: None,
            hours: opening_hours(self.working_time.as_deref()),
        }
    }
}

// -------------------------------------------------------------- availability

#[derive(Debug, Deserialize)]
pub struct AvailabilityEnvelope {
    pub data: Option<AvailabilityData>,
    pub message: Option<String>,
    #[serde(rename = "messageCode")]
    pub message_code: Option<String>,
    #[serde(default, deserialize_with = "loose_bool")]
    pub success: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct AvailabilityData {
    #[serde(rename = "pickupStatus")]
    pub pickup_status: Option<String>,
    #[serde(rename = "stockLevel")]
    pub stock_level: Option<String>,
    pub message: Option<AvailabilityMessage>,
}

#[derive(Debug, Deserialize)]
pub struct AvailabilityMessage {
    pub value: Option<String>,
}

// ---------------------------------------------------------------------- cart

#[derive(Debug, Deserialize)]
pub struct CartEnvelope {
    pub cart: Option<CartNode>,
}

#[derive(Debug, Deserialize)]
pub struct CustomerCartEnvelope {
    #[serde(rename = "customerCart")]
    pub customer_cart: Option<CartIdNode>,
}

#[derive(Debug, Deserialize)]
pub struct CartIdNode {
    pub id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCartEnvelope {
    #[serde(rename = "cartId")]
    pub cart_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CartMutationEnvelope {
    #[serde(alias = "addProductsToCart")]
    #[serde(alias = "updateCartItems")]
    #[serde(alias = "removeItemFromCart")]
    #[serde(alias = "applyCouponsToCart")]
    pub result: Option<CartMutationResult>,
}

#[derive(Debug, Deserialize)]
pub struct CartMutationResult {
    pub cart: Option<CartNode>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub user_errors: Vec<UserError>,
}

#[derive(Debug, Deserialize)]
pub struct UserError {
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CartNode {
    pub id: Option<String>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub total_quantity: Option<f64>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub items: Vec<CartItemNode>,
    pub prices: Option<CartPrices>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub applied_coupons: Vec<CouponNode>,
}

#[derive(Debug, Deserialize)]
pub struct CouponNode {
    pub code: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CartItemNode {
    pub uid: Option<String>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub quantity: Option<f64>,
    pub product: Option<ProductNode>,
    pub prices: Option<CartItemPrices>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub errors: Vec<UserError>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CartItemPrices {
    pub price: Option<MoneyNode>,
    pub price_including_tax: Option<MoneyNode>,
    pub row_total: Option<MoneyNode>,
    pub row_total_including_tax: Option<MoneyNode>,
    pub total_item_discount: Option<MoneyNode>,
}

impl CartItemPrices {
    /// The unit price a person is actually charged.
    ///
    /// `price` is **ex-GST** while the cart's grand total is inclusive, so
    /// showing it puts a number on the line that nobody pays and makes the
    /// cart look like it does not add up -- two towels at `$60.86` in a
    /// `$139.98` cart. The inclusive figure is the shelf price.
    pub fn unit(&self) -> Option<Money> {
        self.price_including_tax
            .as_ref()
            .or(self.price.as_ref())
            .and_then(MoneyNode::money)
    }

    pub fn row(&self) -> Option<Money> {
        self.row_total_including_tax
            .as_ref()
            .or(self.row_total.as_ref())
            .and_then(MoneyNode::money)
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct CartPrices {
    pub grand_total_exclude_gift_card: Option<MoneyNode>,
    pub subtotal_including_tax: Option<MoneyNode>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub discounts: Vec<DiscountNode>,
}

#[derive(Debug, Deserialize)]
pub struct DiscountNode {
    pub label: Option<String>,
    pub amount: Option<MoneyNode>,
}

impl CartNode {
    pub fn cart(&self) -> Cart {
        Cart {
            id: self.id.clone().unwrap_or_default(),
            total_quantity: self.total_quantity.unwrap_or(0.0),
            lines: self.items.iter().map(CartItemNode::line).collect(),
            subtotal: self
                .prices
                .as_ref()
                .and_then(|p| p.subtotal_including_tax.as_ref())
                .and_then(MoneyNode::money),
            total: self
                .prices
                .as_ref()
                .and_then(|p| p.grand_total_exclude_gift_card.as_ref())
                .and_then(MoneyNode::money),
            discounts: self
                .prices
                .iter()
                .flat_map(|p| p.discounts.iter())
                .filter_map(|d| {
                    Some(Discount {
                        label: d.label.clone(),
                        amount: d.amount.as_ref()?.money()?,
                    })
                })
                .collect(),
            coupons: self
                .applied_coupons
                .iter()
                .filter_map(|c| c.code.clone())
                .collect(),
        }
    }
}

impl CartItemNode {
    pub fn line(&self) -> CartLine {
        let product = self.product.as_ref();
        CartLine {
            uid: self.uid.clone().unwrap_or_default(),
            sku: product.and_then(|p| p.sku.clone()).unwrap_or_default(),
            name: product.and_then(|p| p.name.clone()).unwrap_or_default(),
            quantity: self.quantity.unwrap_or(0.0),
            price: self.prices.as_ref().and_then(CartItemPrices::unit),
            row_total: self.prices.as_ref().and_then(CartItemPrices::row),
            in_stock: product.and_then(ProductNode::in_stock),
            errors: self
                .errors
                .iter()
                .filter_map(|e| e.message.clone())
                .collect(),
            fulfilment: product.map(ProductNode::fulfilment).unwrap_or_default(),
        }
    }
}

// ------------------------------------------------------------------ customer

#[derive(Debug, Deserialize)]
pub struct CustomerEnvelope<T> {
    pub customer: Option<T>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CustomerNode {
    pub email: Option<String>,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub customer_group: Option<CustomerGroup>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub custom_attributes: Vec<CustomAttribute>,
}

#[derive(Debug, Deserialize)]
pub struct CustomerGroup {
    #[serde(default, deserialize_with = "loose_string")]
    pub group_code: Option<String>,
}

impl CustomerNode {
    fn attribute(&self, code: &str) -> Option<String> {
        self.custom_attributes
            .iter()
            .find(|a| a.code.as_deref() == Some(code))?
            .value
            .clone()
    }

    pub fn customer(&self) -> Customer {
        // Each fascia files its own type code, so the one to read depends on
        // which store answered. Reading both and preferring whichever is
        // present keeps this from needing the banner.
        let customer_type = self
            .attribute("customer_type_code_briscoes")
            .or_else(|| self.attribute("customer_type_code_rebel"));
        Customer {
            email: self.email.clone(),
            first_name: self.firstname.clone(),
            last_name: self.lastname.clone(),
            customer_type,
            group_code: self
                .customer_group
                .as_ref()
                .and_then(|g| g.group_code.clone()),
            store: self.attribute("briscoes_store_locator"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TokenEnvelope {
    #[serde(alias = "loginGigya")]
    #[serde(alias = "refreshCustomerToken")]
    #[serde(alias = "generateCustomerToken")]
    pub result: Option<TokenNode>,
}

#[derive(Debug, Deserialize)]
pub struct TokenNode {
    pub token: Option<String>,
}

// ------------------------------------------------------------------ wishlist

#[derive(Debug, Default, Deserialize)]
pub struct WishlistCustomer {
    #[serde(default, deserialize_with = "nullable_vec")]
    pub wishlists: Vec<WishlistNode>,
}

#[derive(Debug, Deserialize)]
pub struct WishlistNode {
    #[serde(default, deserialize_with = "loose_string")]
    pub id: Option<String>,
    #[serde(default)]
    pub items_count: u64,
    pub items_v2: Option<WishlistItems>,
}

#[derive(Debug, Default, Deserialize)]
pub struct WishlistItems {
    #[serde(default, deserialize_with = "nullable_vec")]
    pub items: Vec<WishlistItemNode>,
}

#[derive(Debug, Deserialize)]
pub struct WishlistItemNode {
    #[serde(default, deserialize_with = "loose_string")]
    pub id: Option<String>,
    pub product: Option<ProductNode>,
}

#[derive(Debug, Deserialize)]
pub struct WishlistMutationEnvelope {
    #[serde(alias = "addProductsToWishlist")]
    #[serde(alias = "removeProductsFromWishlist")]
    pub result: Option<WishlistMutationResult>,
}

#[derive(Debug, Deserialize)]
pub struct WishlistMutationResult {
    pub wishlist: Option<WishlistNode>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub user_errors: Vec<UserError>,
}

impl WishlistNode {
    pub fn wishlist(&self) -> Wishlist {
        Wishlist {
            id: self.id.clone().unwrap_or_default(),
            count: self.items_count,
            items: self
                .items_v2
                .iter()
                .flat_map(|i| i.items.iter())
                .filter_map(|i| {
                    let product = i.product.as_ref()?;
                    Some(WishlistItem {
                        id: i.id.clone()?,
                        sku: product.sku.clone().unwrap_or_default(),
                        name: product.name.clone().unwrap_or_default(),
                        brand: product.brand.clone(),
                        in_stock: product.in_stock(),
                    })
                })
                .collect(),
        }
    }
}

// -------------------------------------------------------------------- orders

#[derive(Debug, Deserialize)]
pub struct OrderListEnvelope {
    #[serde(rename = "getHistoryOrderList")]
    pub list: Option<OrderList>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OrderList {
    #[serde(default, deserialize_with = "nullable_vec")]
    pub items: Vec<OrderNode>,
    pub page_info: Option<PageInfo>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PageInfo {
    #[serde(default)]
    pub current_page: u64,
    #[serde(default)]
    pub total_pages: u64,
}

#[derive(Debug, Deserialize)]
pub struct OrderNode {
    #[serde(default, deserialize_with = "loose_string")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub number: Option<String>,
    pub order_date: Option<String>,
    pub shipping_code: Option<String>,
    pub shipping_address: Option<OrderAddress>,
    pub custom_attribute_view: Option<OrderStatus>,
    pub total: Option<OrderTotals>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OrderAddress {
    pub store_locator_name: Option<String>,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub city: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OrderStatus {
    pub custom_status: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OrderTotals {
    pub base_grand_total_exclude_gift_card: Option<MoneyNode>,
    pub grand_total: Option<MoneyNode>,
    pub subtotal_incl_tax: Option<MoneyNode>,
    pub shipping_handling: Option<ShippingHandling>,
    pub loyalty_reward: Option<LoyaltyReward>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub discounts: Vec<DiscountNode>,
}

#[derive(Debug, Deserialize)]
pub struct ShippingHandling {
    pub amount_including_tax: Option<MoneyNode>,
}

#[derive(Debug, Deserialize)]
pub struct LoyaltyReward {
    pub amount: Option<MoneyNode>,
}

impl OrderNode {
    pub fn order(&self) -> Option<Order> {
        Some(Order {
            number: self.number.clone()?,
            id: self.id.clone(),
            date: self.order_date.clone(),
            status: self
                .custom_attribute_view
                .as_ref()
                .and_then(|s| s.custom_status.clone()),
            shipping: self.shipping_code.clone(),
            pickup_store: self
                .shipping_address
                .as_ref()
                .and_then(|a| a.store_locator_name.clone())
                .filter(|s| !s.trim().is_empty()),
            total: self
                .total
                .as_ref()
                .and_then(|t| t.base_grand_total_exclude_gift_card.as_ref())
                .and_then(MoneyNode::money),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct OrdersCustomer {
    pub orders: Option<OrdersResult>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OrdersResult {
    #[serde(default, deserialize_with = "nullable_vec")]
    pub items: Vec<OrderDetailNode>,
}

#[derive(Debug, Deserialize)]
pub struct OrderDetailNode {
    #[serde(default, deserialize_with = "loose_string")]
    pub number: Option<String>,
    pub order_date: Option<String>,
    pub custom_attribute_view: Option<OrderStatus>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub custom_shipments: Vec<ShipmentNode>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub payment_methods: Vec<PaymentMethod>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub invoices: Vec<InvoiceNode>,
    pub total: Option<OrderTotals>,
}

#[derive(Debug, Deserialize)]
pub struct PaymentMethod {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InvoiceNode {
    #[serde(default, deserialize_with = "loose_string")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub number: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShipmentNode {
    pub shipment_group_name: Option<String>,
    pub status: Option<String>,
    pub tracking_link: Option<String>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub items: Vec<ShipmentItem>,
}

#[derive(Debug, Deserialize)]
pub struct ShipmentItem {
    pub product_name: Option<String>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub qty: Option<f64>,
    pub product_sale_price: Option<MoneyNode>,
    pub total: Option<MoneyNode>,
}

impl OrderDetailNode {
    pub fn detail(&self) -> Option<OrderDetail> {
        let totals = self.total.as_ref();
        Some(OrderDetail {
            number: self.number.clone()?,
            date: self.order_date.clone(),
            status: self
                .custom_attribute_view
                .as_ref()
                .and_then(|s| s.custom_status.clone()),
            shipments: self
                .custom_shipments
                .iter()
                .map(|s| Shipment {
                    name: s.shipment_group_name.clone(),
                    status: s.status.clone(),
                    tracking: s.tracking_link.clone().filter(|t| !t.trim().is_empty()),
                    lines: s
                        .items
                        .iter()
                        .map(|i| OrderLine {
                            name: i.product_name.clone().unwrap_or_default(),
                            sku: None,
                            quantity: i.qty.unwrap_or(0.0),
                            price: i.product_sale_price.as_ref().and_then(MoneyNode::money),
                            total: i.total.as_ref().and_then(MoneyNode::money),
                        })
                        .collect(),
                })
                .collect(),
            payment_methods: self
                .payment_methods
                .iter()
                .filter_map(|p| p.name.clone())
                .collect(),
            invoices: self
                .invoices
                .iter()
                .filter_map(|i| {
                    Some(Invoice {
                        id: i.id.clone()?,
                        number: i.number.clone(),
                    })
                })
                .collect(),
            total: totals
                .and_then(|t| t.grand_total.as_ref())
                .and_then(MoneyNode::money),
            subtotal: totals
                .and_then(|t| t.subtotal_incl_tax.as_ref())
                .and_then(MoneyNode::money),
            shipping: totals
                .and_then(|t| t.shipping_handling.as_ref())
                .and_then(|s| s.amount_including_tax.as_ref())
                .and_then(MoneyNode::money),
            loyalty_reward: totals
                .and_then(|t| t.loyalty_reward.as_ref())
                .and_then(|l| l.amount.as_ref())
                .and_then(MoneyNode::money),
            discounts: totals
                .iter()
                .flat_map(|t| t.discounts.iter())
                .filter_map(|d| {
                    Some(Discount {
                        label: d.label.clone(),
                        amount: d.amount.as_ref()?.money()?,
                    })
                })
                .collect(),
        })
    }
}

// --------------------------------------------------------- in-store receipts

#[derive(Debug, Default, Deserialize)]
pub struct ReceiptsCustomer {
    #[serde(rename = "customerEEEOrdersHistory")]
    pub history: Option<ReceiptHistory>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReceiptHistory {
    #[serde(default, deserialize_with = "loose_bool")]
    pub success: Option<bool>,
    pub message: Option<String>,
    pub message_type: Option<String>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub items: Vec<ReceiptNode>,
}

#[derive(Debug, Deserialize)]
pub struct ReceiptNode {
    #[serde(default, deserialize_with = "loose_string")]
    pub order_number: Option<String>,
    pub order_location: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub receipt_reference: Option<String>,
    pub order_date: Option<String>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub order_total: Option<f64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    #[serde(rename = "orderLines", default, deserialize_with = "nullable_vec")]
    pub lines: Vec<ReceiptLineNode>,
}

#[derive(Debug, Deserialize)]
pub struct ReceiptLineNode {
    pub product_name: Option<String>,
    #[serde(default, deserialize_with = "loose_string")]
    pub product_code: Option<String>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub quantity: Option<f64>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub row_total: Option<f64>,
}

impl ReceiptNode {
    pub fn receipt(&self) -> Option<Receipt> {
        Some(Receipt {
            number: self.order_number.clone()?,
            store: self.order_location.clone(),
            reference: self.receipt_reference.clone(),
            date: self.order_date.clone(),
            total: self.order_total,
            lines: self
                .lines
                .iter()
                .map(|l| ReceiptLine {
                    name: l.product_name.clone().unwrap_or_default(),
                    code: l.product_code.clone(),
                    quantity: l.quantity.unwrap_or(0.0),
                    total: l.row_total,
                })
                .collect(),
        })
    }
}

/// The storefront says there is older data by the *type* of its message, not by
/// a flag -- `LOAD_MORE_PAST_DATA_MESSAGE_TYPE` beside an otherwise ordinary
/// success.
pub const MORE_HISTORY: &str = "LOAD_MORE_PAST_DATA_MESSAGE_TYPE";

// ------------------------------------------------------------------- loyalty

#[derive(Debug, Default, Deserialize)]
pub struct LoyaltyCustomer {
    #[serde(default, deserialize_with = "loose_bool")]
    pub is_loyalty_eligible: Option<bool>,
    #[serde(default, deserialize_with = "loose_string")]
    pub customer_type_code: Option<String>,
    pub loyalty_info: Option<LoyaltyInfo>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LoyaltyInfo {
    #[serde(default, deserialize_with = "loose_bool")]
    pub is_loyalty_member: Option<bool>,
    #[serde(default, deserialize_with = "loose_string")]
    pub customer_type_code: Option<String>,
    pub balance: Option<LoyaltyBalance>,
    #[serde(default, deserialize_with = "nullable_vec")]
    pub available_vouchers: Vec<VoucherNode>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LoyaltyBalance {
    #[serde(default, deserialize_with = "loose_f64")]
    pub current_spend: Option<f64>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub spend_threshold: Option<f64>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub spend_to_next_reward: Option<f64>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub progress_percentage: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct VoucherNode {
    #[serde(default, deserialize_with = "loose_string")]
    pub voucher_id: Option<String>,
    pub voucher_code: Option<String>,
    pub title: Option<String>,
    pub name: Option<String>,
    pub valid_until: Option<String>,
    #[serde(default, deserialize_with = "loose_bool")]
    pub is_applied: Option<bool>,
}

impl LoyaltyCustomer {
    pub fn loyalty(&self, description: Option<String>) -> Loyalty {
        let info = self.loyalty_info.as_ref();
        let balance = info.and_then(|i| i.balance.as_ref());
        Loyalty {
            member: info.and_then(|i| i.is_loyalty_member).unwrap_or(false),
            eligible: yes(self.is_loyalty_eligible),
            customer_type: info
                .and_then(|i| i.customer_type_code.clone())
                .or_else(|| self.customer_type_code.clone()),
            current_spend: balance.and_then(|b| b.current_spend),
            threshold: balance.and_then(|b| b.spend_threshold),
            to_next_reward: balance.and_then(|b| b.spend_to_next_reward),
            progress: balance.and_then(|b| b.progress_percentage),
            description,
            vouchers: info
                .iter()
                .flat_map(|i| i.available_vouchers.iter())
                .map(|v| Voucher {
                    id: v.voucher_id.clone(),
                    code: v.voucher_code.clone(),
                    title: v.title.clone().or_else(|| v.name.clone()),
                    valid_until: v.valid_until.clone(),
                    applied: yes(v.is_applied),
                })
                .collect(),
        }
    }
}

// --------------------------------------------------------------------- Gigya

/// Gigya answers `200 OK` with an `errorCode` in the body, always.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GigyaResponse {
    #[serde(default)]
    pub error_code: i64,
    pub error_message: Option<String>,
    pub error_details: Option<String>,
    /// The numeric SAP customer id on these sites, not a GUID.
    #[serde(rename = "UID")]
    pub uid: Option<String>,
    #[serde(rename = "UIDSignature")]
    pub uid_signature: Option<String>,
    #[serde(
        rename = "signatureTimestamp",
        default,
        deserialize_with = "loose_string"
    )]
    pub signature_timestamp: Option<String>,
    pub profile: Option<GigyaProfile>,
    #[serde(rename = "sessionInfo")]
    pub session_info: Option<GigyaSession>,
}

#[derive(Debug, Deserialize)]
pub struct GigyaProfile {
    pub email: Option<String>,
    #[serde(rename = "firstName")]
    pub first_name: Option<String>,
    #[serde(rename = "lastName")]
    pub last_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GigyaSession {
    /// The credential worth keeping. Also the value of the site's own
    /// `glt_<apiKey>` cookie, which is why it can be pasted rather than earned.
    pub login_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn de<T: serde::de::DeserializeOwned>(json: &str) -> T {
        serde_json::from_str(json).expect("fixture parses")
    }

    #[test]
    fn a_boolean_survives_arriving_as_an_integer_or_a_word() {
        // Observed in one product answer: `isdropship: 0` beside a Klevu
        // record's `inStock: "yes"`.
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "loose_bool")]
            v: Option<bool>,
        }
        assert_eq!(de::<T>(r#"{"v":0}"#).v, Some(false));
        assert_eq!(de::<T>(r#"{"v":1}"#).v, Some(true));
        assert_eq!(de::<T>(r#"{"v":"yes"}"#).v, Some(true));
        assert_eq!(de::<T>(r#"{"v":"False"}"#).v, Some(false));
        assert_eq!(de::<T>(r#"{"v":null}"#).v, None);
        assert_eq!(de::<T>(r#"{}"#).v, None);
    }

    #[test]
    fn a_price_survives_arriving_quoted() {
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "loose_f64")]
            v: Option<f64>,
        }
        assert_eq!(de::<T>(r#"{"v":"79.99"}"#).v, Some(79.99));
        assert_eq!(de::<T>(r#"{"v":249.99}"#).v, Some(249.99));
        assert_eq!(de::<T>(r#"{"v":"n/a"}"#).v, None);
    }

    #[test]
    fn an_identifier_survives_arriving_as_a_number() {
        // `sapcategory` is "141" on one product and 141 on the next, and the
        // availability API's schema demands a string.
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "loose_string")]
            v: Option<String>,
        }
        assert_eq!(de::<T>(r#"{"v":141}"#).v.as_deref(), Some("141"));
        assert_eq!(de::<T>(r#"{"v":"141"}"#).v.as_deref(), Some("141"));
        assert_eq!(de::<T>(r#"{"v":"  "}"#).v, None, "blank is absent");
    }

    #[test]
    fn a_category_uid_decodes_to_the_id_klevu_indexes_by() {
        // Magento's uid is base64 of the entity id, and Klevu's `ancestors`
        // filter wants the number.
        assert_eq!(decode_uid("MTE4NTA=").as_deref(), Some("11850"));
        assert_eq!(decode_uid("MTQx").as_deref(), Some("141"));
        // A product uid is base64 of a number too, but a uid that decodes to
        // anything else must not be passed off as an id.
        assert_eq!(decode_uid("bm90LWEtbnVtYmVy"), None);
        assert_eq!(decode_uid("!!!"), None);
    }

    #[test]
    fn opening_hours_come_out_of_a_json_document_nested_in_a_string() {
        let raw = r#"[{"record_id":"0","day_of_week":"Monday","open_time":"09:00","close_time":"17:30"},{"record_id":"1","day_of_week":"Sunday","open_time":"","close_time":""}]"#;
        let hours = opening_hours(Some(raw));
        assert_eq!(hours.len(), 2);
        assert_eq!(hours[0].day, "Monday");
        assert_eq!(hours[0].open.as_deref(), Some("09:00"));
        assert_eq!(hours[1].open, None, "an empty time is not a time");
    }

    #[test]
    fn a_malformed_hours_document_costs_the_hours_not_the_store() {
        assert!(opening_hours(Some("not json")).is_empty());
        assert!(opening_hours(None).is_empty());
    }

    #[test]
    fn a_cart_line_is_priced_the_way_the_cart_is_totalled() {
        // Verbatim from a live cart: two towels whose ex-GST line total is
        // $121.72 in a cart whose total is $139.98. Showing the ex-GST figures
        // makes the arithmetic look broken, because for the shopper it is.
        let node: CartItemNode = de(r#"{
            "uid":"OTAzMTc0ODg=",
            "quantity":2,
            "product":{"sku":"1131710","name":"Tudo Home Plain Dyed Hooded Beach Towel"},
            "prices":{
              "price":{"value":60.86,"currency":"NZD"},
              "price_including_tax":{"value":69.99,"currency":"NZD"},
              "row_total":{"value":121.72,"currency":"NZD"},
              "row_total_including_tax":{"value":139.98,"currency":"NZD"}
            }
        }"#);
        let line = node.line();
        assert_eq!(line.price.map(|m| m.value), Some(69.99));
        assert_eq!(line.row_total.map(|m| m.value), Some(139.98));
    }

    #[test]
    fn a_line_with_only_an_ex_gst_price_still_shows_one() {
        // A storefront that stops sending the inclusive field should cost
        // accuracy, not the column.
        let node: CartItemNode = de(r#"{
            "uid":"x","quantity":1,
            "prices":{"price":{"value":60.86},"row_total":{"value":60.86}}
        }"#);
        assert_eq!(node.line().price.map(|m| m.value), Some(60.86));
    }

    #[test]
    fn an_empty_list_may_arrive_as_null_rather_than_as_an_empty_list() {
        // What `bgnz cart list` actually met: an empty cart sends
        // `applied_coupons: null`, and `#[serde(default)]` alone does not
        // cover that -- it only covers the field being absent.
        let node: CartNode = de(r#"{
            "id":"BrN0Z2zCBot1B8e7s71o8XZP0liQc8rC",
            "total_quantity":0,
            "items":null,
            "applied_coupons":null,
            "prices":{"discounts":null}
        }"#);
        let cart = node.cart();
        assert!(cart.lines.is_empty());
        assert!(cart.coupons.is_empty());
        assert!(cart.discounts.is_empty());
    }

    #[test]
    fn a_klevu_record_only_claims_a_was_price_when_there_is_one() {
        let flat: KlevuRecord = de(
            r#"{"sku":"1103832","name":"Jug","price":"79.99","salePrice":"79.99","inStock":"yes"}"#,
        );
        let p = flat.product().expect("a record with a sku is a product");
        assert_eq!(p.price, Some(79.99));
        assert_eq!(p.was_price, None);
        assert_eq!(p.in_stock, Some(true));

        let special: KlevuRecord =
            de(r#"{"sku":"1103832","name":"Jug","price":"79.99","salePrice":"59.99"}"#);
        let p = special.product().expect("a record with a sku is a product");
        assert_eq!(p.was_price, Some(79.99));
        let saving = p.discount().expect("a marked-down record saves");
        assert!((saving - 20.0).abs() < 0.005, "{saving}");
    }

    #[test]
    fn a_product_is_barred_from_collection_by_either_spelling() {
        // The storefront carries two fields meaning the same thing and sets
        // them independently.
        let one: ProductNode = de(r#"{"sku":"1","isclickcollectnotavailable":1}"#);
        assert!(one.fulfilment().click_collect_unavailable);
        let other: ProductNode = de(r#"{"sku":"1","productisclickcollectnotavailable":1}"#);
        assert!(other.fulfilment().click_collect_unavailable);
        let neither: ProductNode = de(r#"{"sku":"1"}"#);
        assert!(!neither.fulfilment().click_collect_unavailable);
    }

    #[test]
    fn a_spec_value_loses_the_markup_it_was_pasted_in_with() {
        // Observed verbatim, unbalanced tag and all.
        assert_eq!(
            strip_tags("</p>Replace the filter after every 35 days</p>"),
            "Replace the filter after every 35 days"
        );
        assert_eq!(strip_tags("2kg&nbsp;net"), "2kg net");
        assert_eq!(strip_tags("<p></p>"), "", "markup alone is not a value");
    }

    #[test]
    fn a_spec_with_nothing_left_after_stripping_is_dropped() {
        let specs: ProductSpecs = de(r#"{
            "key_specs":[{"spec_text":"Care","spec_value":"<p></p>"}],
            "general_specs":[{"spec_text":"SKU","spec_value":"1103832"}]
        }"#);
        let attributes = specs.attributes();
        assert_eq!(attributes.len(), 1);
        assert_eq!(attributes[0].name, "SKU");
    }

    #[test]
    fn a_customer_type_is_read_from_whichever_fascia_filed_one() {
        let node: CustomerNode = de(r#"{
            "email":"shopper@example.invalid",
            "customer_group":{"group_code":"ZW-ZW"},
            "custom_attributes":[
              {"code":"customer_type_code_rebel","value":"L1"},
              {"code":"briscoes_store_locator","value":"1005"}
            ]
        }"#);
        let c = node.customer();
        assert_eq!(c.customer_type.as_deref(), Some("L1"));
        assert_eq!(c.store.as_deref(), Some("1005"));
        assert_eq!(c.group_code.as_deref(), Some("ZW-ZW"));
    }
}
