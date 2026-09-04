//! The JSON The Warehouse actually sends, and the mapping onto [`crate::domain`].
//!
//! Every field optional and every struct `deny_unknown_fields`-free: these
//! payloads carry thirty-odd keys each, most of them analytics or pre-rendered
//! markup, and a new one appearing must not fail a command.

use serde::Deserialize;

use crate::domain::{
    Availability, Cart, CartLine, Category, Price, Product, ProductDetail, ShippingOption, Store,
    VariationAxis, VariationValue,
};

/// Every controller answer carries these, and an action that failed says so in
/// the body with a 200 status -- so the body has to be read either way.
#[derive(Debug, Default, Deserialize)]
pub struct Envelope {
    #[serde(default)]
    pub error: bool,
    /// The site's own words for what went wrong.
    #[serde(default, alias = "errorMessage", alias = "message")]
    pub msg: Option<String>,
}

// ---- money ----

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Money {
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub formatted: Option<String>,
}

impl From<Money> for Price {
    fn from(m: Money) -> Price {
        Price {
            value: m.value,
            formatted: m.formatted,
            currency: m.currency,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PriceBlock {
    #[serde(default)]
    pub sales: Option<Money>,
    /// The pre-discount price, present only when the item is reduced.
    #[serde(default)]
    pub list: Option<Money>,
}

impl PriceBlock {
    pub fn sales(&self) -> Price {
        self.sales.clone().map(Price::from).unwrap_or_default()
    }

    /// The crossed-out price, discarded when it is a hollow object -- the site
    /// sends `{"value":null,"currency":null,"formatted":null}` rather than
    /// omitting it.
    pub fn was(&self) -> Option<Price> {
        let price: Price = self.list.clone()?.into();
        (!price.is_empty()).then_some(price)
    }
}

// ---- products ----

#[derive(Debug, Deserialize)]
pub struct VariationResponse {
    pub product: WireProduct,
}

#[derive(Debug, Default, Deserialize)]
pub struct WireProduct {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "productName")]
    pub product_name: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default, rename = "masterProduct")]
    pub master_product: Option<String>,
    #[serde(default)]
    pub price: PriceBlock,
    #[serde(default)]
    pub availability: Option<WireAvailability>,
    #[serde(default, rename = "variationAttributes")]
    pub variation_attributes: Vec<WireAxis>,
    #[serde(default, rename = "maxOrderQuantity")]
    pub max_order_quantity: Option<u32>,
    #[serde(default, rename = "shortDescription")]
    pub short_description: Option<String>,
    #[serde(default, rename = "longDescription")]
    pub long_description: Option<String>,
    #[serde(default)]
    pub ean: Option<String>,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub images: Option<WireImages>,
    #[serde(default, rename = "shippingOptions")]
    pub shipping_options: Vec<WireShipping>,
    #[serde(default, rename = "selectedProductUrl")]
    pub selected_product_url: Option<String>,
    /// Marketplace items carry their own seller and shipping group.
    #[serde(default, rename = "isMarketplaceProduct")]
    pub marketplace: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct WireImages {
    #[serde(default)]
    pub large: Vec<WireImage>,
    #[serde(default)]
    pub small: Vec<WireImage>,
}

#[derive(Debug, Deserialize)]
pub struct WireImage {
    #[serde(default)]
    pub url: Option<String>,
}

/// The four booleans that make availability two-dimensional.
///
/// `onlineStockAvailable` and `storeChannelOrderable` genuinely disagree: one
/// observed variant had stock in shops and none for delivery, with
/// `productStatus: "FIND_IN_STORE"` saying so.
#[derive(Debug, Default, Deserialize)]
pub struct WireAvailability {
    #[serde(default, rename = "onlineStockAvailable")]
    pub online_stock_available: Option<bool>,
    #[serde(default, rename = "onlineChannelOrderable")]
    pub online_channel_orderable: Option<bool>,
    #[serde(default, rename = "storeChannelOrderable")]
    pub store_channel_orderable: Option<bool>,
    #[serde(default, rename = "productStatus")]
    pub product_status: Option<String>,
    #[serde(default, rename = "cartLabel")]
    pub cart_label: Option<String>,
}

impl From<WireAvailability> for Availability {
    fn from(w: WireAvailability) -> Availability {
        Availability {
            status: w.product_status,
            // Orderable is the stricter of the two and the one that matters:
            // stock that exists but is not sellable online is not online stock.
            online: match (w.online_channel_orderable, w.online_stock_available) {
                (Some(o), Some(s)) => Some(o && s),
                (o, s) => o.or(s),
            },
            in_store: w.store_channel_orderable,
            label: w.cart_label,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WireAxis {
    #[serde(default, rename = "attributeId")]
    pub attribute_id: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(default, rename = "displayValue")]
    pub display_value: Option<String>,
    #[serde(default)]
    pub values: Vec<WireAxisValue>,
}

#[derive(Debug, Deserialize)]
pub struct WireAxisValue {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "displayValue")]
    pub display_value: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub selected: bool,
    /// Whether the combination exists at all -- a colour may simply not be made
    /// in the chosen size.
    #[serde(default)]
    pub selectable: bool,
    /// Whether it can be bought. A real combination can still be sold out, so
    /// this is a different question from `selectable`.
    #[serde(default)]
    pub orderable: bool,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WireShipping {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(default, rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(default, rename = "estimatedArrivalTime")]
    pub estimated_arrival_time: Option<String>,
    #[serde(default, rename = "storePickupEnabled")]
    pub store_pickup_enabled: bool,
}

impl WireProduct {
    pub fn into_product(self) -> Product {
        let id = self.id.clone().unwrap_or_default();
        Product {
            master_id: self.master_product.clone(),
            name: self.product_name.clone().unwrap_or_default(),
            brand: self.brand.clone(),
            ean: self.ean.clone(),
            price: self.price.sales(),
            was_price: self.price.was(),
            rating: self.rating,
            category: None,
            url: self.selected_product_url.clone(),
            image: self
                .images
                .as_ref()
                .and_then(|i| i.large.first().or_else(|| i.small.first()))
                .and_then(|i| i.url.clone()),
            availability: self
                .availability
                .map(Availability::from)
                .unwrap_or_default(),
            marketplace: self.marketplace.unwrap_or(false),
            id,
        }
    }

    pub fn into_detail(self) -> ProductDetail {
        // The long description is a fragment of HTML; the short one is a plain
        // sentence. Both go through the same renderer, which leaves plain text
        // untouched.
        let description = self
            .long_description
            .clone()
            .or_else(|| self.short_description.clone())
            .map(|d| crate::extract::plain_text(&unescape(&d)))
            .filter(|d| !d.is_empty());
        let max_quantity = self.max_order_quantity;
        let axes = self
            .variation_attributes
            .iter()
            .filter_map(|a| {
                let id = a.attribute_id.clone().or_else(|| a.id.clone())?;
                Some(VariationAxis {
                    name: a.display_name.clone().unwrap_or_else(|| id.clone()),
                    selected: a
                        .display_value
                        .clone()
                        .filter(|v| !v.is_empty())
                        .or_else(|| {
                            a.values
                                .iter()
                                .find(|v| v.selected)
                                .and_then(|v| v.display_value.clone())
                        }),
                    values: a
                        .values
                        .iter()
                        .filter_map(|v| {
                            let vid = v.id.clone().or_else(|| v.value.clone())?;
                            Some(VariationValue {
                                label: v.display_value.clone().unwrap_or_else(|| vid.clone()),
                                selected: v.selected,
                                selectable: v.selectable,
                                orderable: v.orderable,
                                url: v.url.clone(),
                                id: vid,
                            })
                        })
                        .collect(),
                    id,
                })
            })
            .collect();
        let shipping = self
            .shipping_options
            .iter()
            .map(|s| ShippingOption {
                id: s.id.clone(),
                name: s.display_name.clone().unwrap_or_else(|| s.id.clone()),
                estimate: s.estimated_arrival_time.clone(),
                pickup: s.store_pickup_enabled,
            })
            .collect();
        ProductDetail {
            sku: self.id.clone(),
            product: self.into_product(),
            description,
            max_quantity,
            axes,
            shipping,
        }
    }
}

/// The site double-escapes descriptions into JSON -- `H&amp;H` arrives as
/// written -- so an entity that survived has to be undone once for printing.
pub(crate) fn unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

// ---- cart ----

/// One basket under five names.
///
/// `Cart-UpdateQuantity` calls it `cartModel`, `Cart-AddProduct` calls it
/// `cart`, `Cart-SelectStore` calls it `basketModel`, and
/// `Cart-RemoveProductLineItem` calls it `basket` -- while `Cart-MiniCartShow`
/// skips the wrapper and puts the fields at the top level. The models are
/// otherwise byte-for-byte the same shape.
///
/// All five are accepted rather than one being picked and the rest reading as
/// an empty basket, which is what "Removed it, and the cart is empty" was.
#[derive(Debug, Default, Deserialize)]
pub struct CartResponse {
    #[serde(default, rename = "cartModel")]
    pub cart_model: Option<WireCart>,
    #[serde(default)]
    pub cart: Option<WireCart>,
    #[serde(default, rename = "basketModel")]
    pub basket_model: Option<WireCart>,
    #[serde(default)]
    pub basket: Option<WireCart>,
    /// The minicart's own fields, and the count the write controllers repeat
    /// beside their model.
    #[serde(default, rename = "quantityTotal")]
    pub quantity_total: Option<u32>,
    #[serde(default)]
    pub items: Option<Vec<WireCartItem>>,
    #[serde(default, rename = "subTotal")]
    pub sub_total: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
pub struct WireCart {
    #[serde(default, rename = "cartId")]
    pub cart_id: Option<String>,
    #[serde(default)]
    pub items: Vec<WireCartItem>,
    /// Only the minicart puts a subtotal here. Every wrapped model keeps it in
    /// `totals` instead, so reading one name and not the other is a basket that
    /// lists its lines and then claims no total.
    #[serde(default, rename = "subTotal")]
    pub sub_total: Option<serde_json::Value>,
    #[serde(default)]
    pub totals: Option<WireTotals>,
    #[serde(default, rename = "quantityTotal")]
    pub quantity_total: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct WireTotals {
    #[serde(default, rename = "subTotal")]
    pub sub_total: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct WireCartItem {
    /// The line id. The same product added twice is two lines, so removing and
    /// re-quantifying take this rather than the product id.
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "productName")]
    pub product_name: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    /// The line id removal takes, under the two names the site gives it.
    ///
    /// Two fields rather than one with an alias: the minicart sends *both* on
    /// the same item, and an alias makes that a duplicate-field error that
    /// fails the whole cart.
    #[serde(default, rename = "UUID")]
    pub uuid_upper: Option<String>,
    #[serde(default, rename = "preOrderUUID")]
    pub pre_order_uuid: Option<String>,
    #[serde(default)]
    pub quantity: Option<u32>,
    #[serde(default)]
    pub price: Option<PriceBlock>,
    /// The line total, in the two shapes the site sends it: flat money from the
    /// minicart, and a nested price block from every wrapped model.
    #[serde(default, rename = "priceTotal")]
    pub price_total: Option<WireLineTotal>,
}

/// `priceTotal` is `{"value":…,"formatted":…}` in the minicart and
/// `{"price":{"sales":{…}}}` in the cart models. Typing it as one of them
/// silently dropped the other, which is a line total that renders as a dash
/// beside three that do not.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WireLineTotal {
    Nested { price: PriceBlock },
    Flat(Money),
}

impl From<WireLineTotal> for Price {
    fn from(t: WireLineTotal) -> Price {
        match t {
            WireLineTotal::Nested { price } => price.sales(),
            WireLineTotal::Flat(m) => Price::from(m),
        }
    }
}

impl From<WireCartItem> for CartLine {
    fn from(w: WireCartItem) -> CartLine {
        let quantity = w.quantity.unwrap_or(1);
        // `price.sales` is per unit; `priceTotal` is the line. Whichever
        // arrived, the other is derived rather than left empty -- and never
        // conflated, because a "Price" column that means one thing after `add`
        // and another after `list` is worse than a missing one.
        let unit = w.price.as_ref().map(PriceBlock::sales);
        let total = w.price_total.clone().map(Price::from);
        CartLine {
            uuid: w.uuid.clone().unwrap_or_default(),
            pli_uuid: w.uuid_upper.clone().or_else(|| w.pre_order_uuid.clone()),
            id: w.id.clone().unwrap_or_default(),
            name: w.product_name.clone().unwrap_or_default(),
            brand: w.brand.clone(),
            price: unit
                .clone()
                .or_else(|| total.as_ref().and_then(|t| scale(t, 1.0 / quantity as f64)))
                .unwrap_or_default(),
            total: total
                .or_else(|| unit.as_ref().and_then(|u| scale(u, quantity as f64)))
                .unwrap_or_default(),
            quantity,
        }
    }
}

/// A price times a factor, for deriving the half the site did not send.
///
/// Only the number: the formatted string it came with is the site's own words
/// for a different amount, so carrying it across would print a lie.
fn scale(price: &Price, factor: f64) -> Option<Price> {
    let value = price.value? * factor;
    Some(Price {
        value: Some(value),
        formatted: Some(format!("${value:.2}")),
        currency: price.currency.clone(),
    })
}

impl CartResponse {
    pub fn into_cart(self) -> Cart {
        let model = self
            .cart_model
            .or(self.cart)
            .or(self.basket_model)
            .or(self.basket);
        let (id, items, subtotal, quantity) = match model {
            Some(m) => (
                m.cart_id,
                m.items,
                m.sub_total
                    .or_else(|| m.totals.and_then(|t| t.sub_total))
                    .or(self.sub_total),
                // The model's own count, ahead of the one beside it: a removal
                // repeats `quantityTotal` at the top level for the line it just
                // took out, which is always zero.
                m.quantity_total.or(self.quantity_total),
            ),
            None => (
                None,
                self.items.unwrap_or_default(),
                self.sub_total,
                self.quantity_total,
            ),
        };
        let lines: Vec<CartLine> = items.into_iter().map(CartLine::from).collect();
        Cart {
            id,
            // The site's own total when it sent one, because it counts units
            // rather than lines and a bundle can contribute more than one.
            quantity: quantity.unwrap_or_else(|| lines.iter().map(|l| l.quantity).sum()),
            subtotal: subtotal.and_then(|v| match v {
                serde_json::Value::String(s) => Some(s),
                serde_json::Value::Number(n) => Some(format!("${n}")),
                serde_json::Value::Object(o) => o
                    .get("formatted")
                    .and_then(|f| f.as_str())
                    .map(str::to_string),
                _ => None,
            }),
            lines,
        }
    }
}

// ---- stores ----

#[derive(Debug, Deserialize)]
pub struct StoresResponse {
    pub stores: StoresInner,
}

#[derive(Debug, Deserialize)]
pub struct StoresInner {
    #[serde(default)]
    pub stores: Vec<WireStore>,
}

#[derive(Debug, Deserialize)]
pub struct WireStore {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "fullAddress")]
    pub full_address: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default, rename = "postalCode")]
    pub postal_code: Option<String>,
    #[serde(default, rename = "stateCode")]
    pub state_code: Option<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default, rename = "isClickAndCollectSupported")]
    pub click_and_collect: Option<bool>,
    #[serde(default, rename = "isOpenNow")]
    pub open_now: Option<bool>,
    #[serde(default, rename = "openingHoursJson")]
    pub opening_hours: Option<WireHours>,
}

#[derive(Debug, Deserialize)]
pub struct WireHours {
    #[serde(default, rename = "openingHours")]
    pub opening_hours: Option<String>,
}

impl From<WireStore> for Store {
    fn from(w: WireStore) -> Store {
        Store {
            name: w.name.clone().unwrap_or_else(|| w.id.clone()),
            address: w.full_address.clone(),
            city: w.city.clone(),
            postcode: w.postal_code.clone(),
            region: w.state_code.clone(),
            latitude: w.latitude,
            longitude: w.longitude,
            phone: w.phone.clone(),
            email: w.email.clone(),
            click_and_collect: w.click_and_collect,
            hours_today: w.opening_hours.and_then(|h| h.opening_hours),
            open_now: w.open_now,
            id: w.id,
        }
    }
}

// ---- taxonomy ----

#[derive(Debug, Deserialize)]
pub struct CategoriesResponse {
    #[serde(default)]
    pub categories: Vec<WireCategory>,
}

#[derive(Debug, Deserialize)]
pub struct WireCategory {
    /// Absent when the entry is a refusal rather than a category.
    ///
    /// `Category-GetMultipleNavigationHierarchy` answers per requested id and
    /// puts `{"error":true,"msg":"Unable to find menu category"}` in the array
    /// where one is unknown. Requiring an `id` here failed the whole tree
    /// because of one stale root, which is the wrong trade: a department that
    /// has been renamed should cost that department.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// `maxDepth=0` answers without these, so a tree needs a request per
    /// level rather than one call.
    #[serde(default, rename = "subCategories")]
    pub sub_categories: Vec<WireCategory>,
}

impl WireCategory {
    /// The category this describes, or `None` when the entry was a refusal.
    pub fn into_category(self) -> Option<Category> {
        let id = self.id.clone()?;
        Some(Category {
            name: self.display_name.clone().unwrap_or_else(|| id.clone()),
            // The URL is absolute; the path is what a landing page is fetched
            // by and what a person recognises.
            path: self.url.as_deref().and_then(|u| {
                let after = u.split("://").nth(1)?;
                let path = after.split_once('/').map(|(_, p)| p)?;
                Some(format!("/{path}"))
            }),
            children: self
                .sub_categories
                .into_iter()
                .filter_map(WireCategory::into_category)
                .collect(),
            id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hollow_list_price_is_not_a_discount() {
        // The site sends the object with every field null rather than omitting
        // it, which would otherwise render as an empty "was" column on every
        // full-price item.
        let block: PriceBlock = serde_json::from_str(
            r#"{"sales":{"value":7.49,"currency":"NZD","formatted":"$7.49"},
                "list":{"value":null,"currency":null,"formatted":null}}"#,
        )
        .unwrap();
        assert_eq!(block.sales().value, Some(7.49));
        assert_eq!(block.was(), None);
    }

    #[test]
    fn a_real_list_price_survives() {
        let block: PriceBlock = serde_json::from_str(
            r#"{"sales":{"formatted":"$5.00"},"list":{"value":9.0,"formatted":"$9.00"}}"#,
        )
        .unwrap();
        assert_eq!(block.was().unwrap().formatted.as_deref(), Some("$9.00"));
    }

    #[test]
    fn stock_that_exists_but_cannot_be_sold_online_is_not_online_stock() {
        let w: WireAvailability = serde_json::from_str(
            r#"{"onlineStockAvailable":true,"onlineChannelOrderable":false,
                "storeChannelOrderable":true,"productStatus":"FIND_IN_STORE",
                "cartLabel":"In-store only"}"#,
        )
        .unwrap();
        let a = Availability::from(w);
        assert_eq!(a.online, Some(false), "orderable is the stricter test");
        assert_eq!(a.in_store, Some(true));
        assert_eq!(a.summary(), "in store");
    }

    #[test]
    fn a_variation_axis_keeps_selectable_and_orderable_apart() {
        // A size can be a real combination and still be sold out. Collapsing
        // the two would either hide sizes that exist or offer ones that cannot
        // be bought.
        let p: WireProduct = serde_json::from_str(
            r#"{"id":"RM1-8M","productName":"Tee","masterProduct":"RM1",
                "variationAttributes":[{"attributeId":"size","displayName":"Size","values":[
                  {"id":"XS","displayValue":"XS","selectable":false,"orderable":false},
                  {"id":"S","displayValue":"S","selectable":true,"orderable":false},
                  {"id":"M","displayValue":"M","selectable":true,"orderable":true,"selected":true}]}]}"#,
        )
        .unwrap();
        let d = p.into_detail();
        let size = &d.axes[0];
        assert_eq!(size.id, "size");
        assert_eq!(size.selected.as_deref(), Some("M"));
        assert_eq!(size.values.len(), 3);
        assert!(!size.values[0].selectable);
        assert!(size.values[1].selectable && !size.values[1].orderable);
        assert!(size.values[2].orderable);
    }

    #[test]
    fn an_html_entity_that_survived_into_json_is_undone_once() {
        let p: WireProduct =
            serde_json::from_str(r#"{"id":"R1","shortDescription":"H&amp;H Men&#39;s Tee"}"#)
                .unwrap();
        assert_eq!(
            p.into_detail().description.as_deref(),
            Some("H&H Men's Tee")
        );
    }

    #[test]
    fn an_add_nests_its_cart_under_a_name_of_its_own() {
        // `Cart-AddProduct` says `cart` where the others say `cartModel`.
        // Missing it is not loud: the add succeeds and the basket reads as
        // empty, which is exactly the wrong thing to show after adding.
        let parsed: CartResponse = serde_json::from_str(
            r#"{"error":false,"message":"Product added to cart","quantityTotal":5,
                "cart":{"cartId":"c1","items":[
                  {"uuid":"u1","id":"R1","productName":"Thing","quantity":2,
                   "price":{"sales":{"value":7.49,"formatted":"$7.49"}}}]}}"#,
        )
        .unwrap();
        let cart = parsed.into_cart();
        assert_eq!(cart.lines.len(), 1);
        assert_eq!(cart.id.as_deref(), Some("c1"));
        // The site's own count, which is units across the basket rather than
        // what this one response happened to carry.
        assert_eq!(cart.quantity, 5);
    }

    #[test]
    fn both_cart_shapes_read_as_one_cart() {
        // `Cart-UpdateQuantity` nests under `cartModel`; `Cart-MiniCartShow`
        // puts the same fields at the top level.
        let nested: CartResponse = serde_json::from_str(
            r#"{"error":false,"cartModel":{"cartId":"c1","subTotal":"$10.48","items":[
                {"uuid":"u1","id":"R1","productName":"Thing","quantity":2,
                 "price":{"sales":{"value":7.49,"formatted":"$7.49"}}}]}}"#,
        )
        .unwrap();
        let cart = nested.into_cart();
        assert_eq!(cart.id.as_deref(), Some("c1"));
        assert_eq!(cart.lines[0].uuid, "u1");
        assert_eq!(cart.quantity, 2, "units, counted off the lines");
        assert_eq!(cart.subtotal.as_deref(), Some("$10.48"));

        let flat: CartResponse = serde_json::from_str(
            r#"{"quantityTotal":2,"subTotal":"$10.48","items":[
                {"uuid":"u1","id":"R1","productName":"Thing","quantity":1,
                 "priceTotal":{"value":7.49,"formatted":"$7.49"}},
                {"uuid":"u2","id":"R2","productName":"Other","quantity":1,
                 "priceTotal":{"value":2.99,"formatted":"$2.99"}}]}"#,
        )
        .unwrap();
        let cart = flat.into_cart();
        assert_eq!(cart.lines.len(), 2);
        assert_eq!(cart.quantity, 2);
        assert_eq!(cart.lines[0].price.formatted.as_deref(), Some("$7.49"));
    }

    #[test]
    fn every_name_the_site_gives_the_cart_reads_as_a_cart() {
        // Five controllers, five names. Missing one is quiet and wrong: the
        // action succeeds and the basket reads as empty.
        for key in ["cartModel", "cart", "basketModel", "basket"] {
            let body = format!(
                r#"{{"error":false,"{key}":{{"cartId":"c1","items":[
                   {{"uuid":"u","id":"R1","productName":"T","quantity":1}}]}}}}"#
            );
            let parsed: CartResponse = serde_json::from_str(&body).unwrap();
            assert_eq!(parsed.into_cart().lines.len(), 1, "{key}");
        }
    }

    #[test]
    fn a_wrapped_basket_takes_its_totals_over_the_envelope_around_it() {
        // A removal answers with the basket that survived it, and repeats
        // `quantityTotal` at the top level for the line it just took out --
        // where it is always zero. Reading the outer one empties the basket on
        // paper; reading no `totals` leaves it without a subtotal.
        let parsed: CartResponse = serde_json::from_str(
            r#"{"basket":{"items":[{"uuid":"u","UUID":"p","id":"R1","productName":"T",
                "quantity":2}],"quantityTotal":2,"totals":{"subTotal":"$14.98"}},
                "quantityTotal":0,"updateQuantity":0}"#,
        )
        .unwrap();
        let cart = parsed.into_cart();
        assert_eq!(cart.lines.len(), 1);
        assert_eq!(cart.quantity, 2);
        assert_eq!(cart.subtotal.as_deref(), Some("$14.98"));
    }

    #[test]
    fn a_line_total_reads_in_both_of_the_shapes_it_arrives_in() {
        // Flat from the minicart, nested from every wrapped model. Typing it as
        // one dropped the other, and a dropped total is derived from the unit
        // price -- right at quantity one, wrong everywhere else.
        let flat: CartResponse = serde_json::from_str(
            r#"{"items":[{"id":"R1","productName":"T","quantity":2,
                "price":{"sales":{"value":7.49,"formatted":"$7.49"}},
                "priceTotal":{"value":14.98,"formatted":"$14.98"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            flat.into_cart().lines[0].total.formatted.as_deref(),
            Some("$14.98")
        );

        let nested: CartResponse = serde_json::from_str(
            r#"{"cart":{"items":[{"id":"R1","productName":"T","quantity":2,
                "price":{"sales":{"value":7.49,"formatted":"$7.49"}},
                "priceTotal":{"price":{"sales":{"value":14.98,"formatted":"$14.98"}}}}]}}"#,
        )
        .unwrap();
        assert_eq!(
            nested.into_cart().lines[0].total.formatted.as_deref(),
            Some("$14.98")
        );
    }

    #[test]
    fn a_line_carries_both_of_the_ids_the_site_gives_it() {
        // Removal takes the second one, and refuses the first. Reading only
        // `uuid` is why `cart remove` used to report a success it had not had.
        let parsed: CartResponse = serde_json::from_str(
            r#"{"items":[{"uuid":"line-1","UUID":"pli-1","id":"R1",
                "productName":"T","quantity":1}]}"#,
        )
        .unwrap();
        let line = &parsed.into_cart().lines[0];
        assert_eq!(line.uuid, "line-1");
        assert_eq!(line.pli_uuid.as_deref(), Some("pli-1"));

        // `preOrderUUID` is the same value under the name the other
        // controllers use -- and the minicart sends both at once, which an
        // aliased single field would reject outright.
        let both: CartResponse = serde_json::from_str(
            r#"{"items":[{"uuid":"line-1","UUID":"pli-1","preOrderUUID":"pli-1","id":"R1",
                "productName":"T","quantity":1}]}"#,
        )
        .unwrap();
        assert_eq!(both.into_cart().lines[0].pli_uuid.as_deref(), Some("pli-1"));

        let only_pre: CartResponse = serde_json::from_str(
            r#"{"cartModel":{"items":[{"uuid":"line-1","preOrderUUID":"pli-1","id":"R1",
                "productName":"T","quantity":1}]}}"#,
        )
        .unwrap();
        assert_eq!(
            only_pre.into_cart().lines[0].pli_uuid.as_deref(),
            Some("pli-1")
        );
    }

    #[test]
    fn a_unit_price_and_a_line_total_are_never_confused() {
        // `Cart-AddProduct` sends the unit price, `Cart-MiniCartShow` the line
        // total. Whichever arrives, both are reported.
        let from_add: CartResponse = serde_json::from_str(
            r#"{"cart":{"items":[{"uuid":"u","id":"R1","productName":"T","quantity":4,
                "price":{"sales":{"value":9.99,"formatted":"$9.99"}}}]}}"#,
        )
        .unwrap();
        let line = &from_add.into_cart().lines[0];
        assert_eq!(line.price.value, Some(9.99), "per unit, as sent");
        assert_eq!(line.total.formatted.as_deref(), Some("$39.96"), "derived");

        let from_minicart: CartResponse = serde_json::from_str(
            r#"{"items":[{"uuid":"u","id":"R1","productName":"T","quantity":2,
                "priceTotal":{"value":19.98,"formatted":"$19.98"}}]}"#,
        )
        .unwrap();
        let line = &from_minicart.into_cart().lines[0];
        assert_eq!(line.total.value, Some(19.98), "the line, as sent");
        assert_eq!(line.price.formatted.as_deref(), Some("$9.99"), "derived");
    }

    #[test]
    fn a_category_url_becomes_the_path_a_landing_page_is_fetched_by() {
        let w: WireCategory = serde_json::from_str(
            r#"{"id":"homegarden","displayName":"Home, Garden & Appliances",
                "url":"https://www.thewarehouse.co.nz/c/home-garden-appliances"}"#,
        )
        .unwrap();
        let c = w.into_category().unwrap();
        assert_eq!(c.id, "homegarden");
        assert_eq!(c.path.as_deref(), Some("/c/home-garden-appliances"));
        assert_eq!(c.name, "Home, Garden & Appliances");
    }

    #[test]
    fn a_refused_category_is_dropped_rather_than_failing_the_whole_tree() {
        // The endpoint answers per requested id and puts a refusal in the array
        // where one is unknown. One stale root should cost that department,
        // not `twlnz departments`.
        let parsed: CategoriesResponse = serde_json::from_str(
            r#"{"categories":[
                {"action":"Category-GetNavigationHierarchy","id":"toysbaby","displayName":"Toys & Baby"},
                {"action":"Category-GetNavigationHierarchy","error":true,"msg":"Unable to find menu category"}]}"#,
        )
        .unwrap();
        let categories: Vec<Category> = parsed
            .categories
            .into_iter()
            .filter_map(WireCategory::into_category)
            .collect();
        assert_eq!(categories.len(), 1);
        assert_eq!(categories[0].id, "toysbaby");
    }

    #[test]
    fn a_store_keeps_the_region_it_was_found_by() {
        let w: WireStore = serde_json::from_str(
            r#"{"ID":"119","name":"Example Town","fullAddress":"1 Example Road, Example Town",
                "stateCode":"NZ-AUK","latitude":-36.7,"longitude":174.7,
                "isClickAndCollectSupported":true,"isOpenNow":true,
                "openingHoursJson":{"openingHours":"8.00am - 9.00pm"}}"#,
        )
        .unwrap();
        let s = Store::from(w);
        assert_eq!(s.id, "119");
        assert_eq!(s.region.as_deref(), Some("NZ-AUK"));
        assert_eq!(s.hours_today.as_deref(), Some("8.00am - 9.00pm"));
        assert_eq!(s.click_and_collect, Some(true));
    }
}
