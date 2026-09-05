//! What the wire actually carries.
//!
//! Two undocumented JSON dialects, so the rule here is that **nothing is
//! required**. Every field is `Option` or defaults, because a field either
//! vendor renames should cost a column rather than the whole command.
//!
//! Constructor's product record is the sharp edge. It is a search index rather
//! than a product API, and the `data` bag is populated per category by
//! whatever the merchandising feed happened to carry: `Capacity` and
//! `Power Rating` exist on an appliance and not on a t-shirt, `Size` is
//! sometimes `"One Size"` and sometimes `"L"`, and `apn` is a number in one
//! record and absent in the next. So the typed fields below are only the ones
//! observed on every product, and the rest stays in `extra` as raw JSON for a
//! caller that wants it.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};

/// A number that is sometimes quoted.
///
/// The gateway sends a store's latitude as the string
/// `"-36.914257999999997"` and its `distanceInKm` as the number `16.6`, in the
/// same response. Typing either as `f64` alone breaks on the other, and a
/// coordinate that fails to parse should cost a column rather than the whole
/// store.
fn loose_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}

// ---------------------------------------------------------------- Constructor

#[derive(Debug, Deserialize)]
pub struct SearchEnvelope {
    #[serde(default)]
    pub response: SearchResponse,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub results: Vec<SearchResult>,
    #[serde(default)]
    pub facets: Vec<Facet>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub sort_options: Vec<SortOption>,
    #[serde(default)]
    pub total_num_results: u64,
    /// How the results were found. `token_match` is a real keyword hit;
    /// `embeddings_match` is the semantic fallback, which is why a nonsense
    /// term still answers with twenty products rather than none.
    #[serde(default)]
    pub result_sources: BTreeMap<String, ResultSource>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ResultSource {
    #[serde(default)]
    pub count: u64,
}

#[derive(Debug, Deserialize)]
pub struct SearchResult {
    /// The display name. Constructor's own field for it, oddly spelled.
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub data: ItemData,
    /// A product's sizes and colours. Each carries its own keycode, which is
    /// what the cart wants -- the parent record's keycode is only one of them.
    #[serde(default)]
    pub variations: Vec<Variation>,
}

#[derive(Debug, Deserialize)]
pub struct Variation {
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub data: ItemData,
}

/// The `data` bag. Typed where it is reliable, raw where it is not.
#[derive(Debug, Default, Deserialize)]
pub struct ItemData {
    /// `P_43165537`. The keycode with a prefix; [`ItemData::variation_id`] is
    /// the one to use.
    #[serde(default)]
    pub id: Option<String>,
    /// The keycode -- the id everything else in this crate speaks.
    #[serde(default)]
    pub variation_id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(rename = "Brand", default)]
    pub brand: Option<String>,
    #[serde(rename = "Colour", default)]
    pub colour: Option<String>,
    #[serde(rename = "Size", default)]
    pub size: Option<String>,
    /// A marketplace item names its seller here; a Kmart-sold one says
    /// `["Kmart"]`. An array because an item can be offered by more than one.
    #[serde(rename = "Seller", default)]
    pub seller: Vec<String>,
    #[serde(rename = "FreeShipping", default)]
    pub free_shipping: Option<bool>,
    /// Whether stock is pooled nationally rather than held per store. The
    /// availability query needs this, and it is only ever found here.
    #[serde(rename = "nationalInventory", default)]
    pub national_inventory: Option<bool>,
    #[serde(rename = "primaryCategoryId", default)]
    pub primary_category_id: Option<String>,
    /// Dollars, as a number. The effective price -- what the site renders.
    #[serde(default)]
    pub price: Option<f64>,
    /// The priced entries behind it, which is where a currency is stated.
    #[serde(default)]
    pub prices: Vec<PriceEntry>,
    #[serde(default)]
    pub ratings: Option<Ratings>,
    /// HTML. Kmart writes product copy as markup, so this is not printable
    /// as-is.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "altImages", default)]
    pub alt_images: Vec<String>,
    /// Everything else the feed carried. Per-category and unstable, which is
    /// exactly why it is not typed.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct PriceEntry {
    /// `list` is the only kind observed. A second kind is what a markdown
    /// would arrive as, so it is read rather than assumed absent.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// A decimal string, `"39.00"` -- not a number, and not cents.
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ratings {
    #[serde(default)]
    pub average_score: Option<f64>,
    #[serde(default)]
    pub total_reviews: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct Facet {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub options: Vec<FacetOption>,
}

#[derive(Debug, Deserialize)]
pub struct FacetOption {
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SortOption {
    #[serde(default)]
    pub sort_by: Option<String>,
    #[serde(default)]
    pub sort_order: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// A category, as the index knows it. Recursive: one request can return the
/// whole tree.
#[derive(Debug, Deserialize)]
pub struct Group {
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub children: Vec<Group>,
    #[serde(default)]
    pub data: GroupData,
}

#[derive(Debug, Default, Deserialize)]
pub struct GroupData {
    /// The storefront path, which is the only human-readable handle a category
    /// has.
    #[serde(default)]
    pub url: Option<String>,
}

// -------------------------------------------------------------- the gateway

/// A GraphQL answer. `data` and `errors` can both be present: the gateway
/// answers `200` with a partial result and an error list rather than a status,
/// which is why nothing here reads the status to decide whether a call worked.
#[derive(Debug, Deserialize)]
pub struct GqlEnvelope<T> {
    #[serde(default = "Option::default")]
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Vec<GqlError>,
}

#[derive(Debug, Deserialize)]
pub struct GqlError {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl GqlError {
    /// The `extensions.code`, which is where the gateway states the machine
    /// -readable half of a refusal -- `UNAUTHENTICATED` and the like.
    pub fn code(&self) -> Option<&str> {
        self.extensions.get("code")?.as_str()
    }
}

#[derive(Debug, Deserialize)]
pub struct AvailabilityData {
    #[serde(rename = "getProductAvailability", default)]
    pub result: Option<AvailabilityResult>,
}

#[derive(Debug, Deserialize)]
pub struct AvailabilityResult {
    #[serde(default)]
    pub postcode: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub availability: Option<Fulfilment>,
}

/// The four channels. Every one is nullable and they are nulled independently:
/// a product can be orderable and not collectable, or the reverse.
#[derive(Debug, Default, Deserialize)]
pub struct Fulfilment {
    #[serde(rename = "HOME_DELIVERY", default)]
    pub home_delivery: Option<Vec<ChannelStock>>,
    #[serde(rename = "CLICK_AND_COLLECT", default)]
    pub click_and_collect: Option<Vec<CncStock>>,
    #[serde(rename = "IN_STORE", default)]
    pub in_store: Option<Vec<CncStock>>,
    #[serde(rename = "EXPRESS_DELIVERY", default)]
    pub express: Option<Vec<ChannelStock>>,
}

/// `HOME_DELIVERY` and `EXPRESS_DELIVERY`: one pooled number, no buildings.
#[derive(Debug, Deserialize)]
pub struct ChannelStock {
    #[serde(rename = "poolName", default)]
    pub pool_name: Option<String>,
    #[serde(default)]
    pub stock: Option<Available>,
}

#[derive(Debug, Deserialize)]
pub struct Available {
    #[serde(default)]
    pub available: Option<i64>,
}

/// `CLICK_AND_COLLECT` and `IN_STORE`, which share a shape: a keycode and a
/// list of buildings. Only the former carries a pooled total.
#[derive(Debug, Deserialize)]
pub struct CncStock {
    #[serde(default)]
    pub stock: Option<TotalStock>,
    #[serde(default)]
    pub locations: Vec<CncLocation>,
}

#[derive(Debug, Deserialize)]
pub struct TotalStock {
    #[serde(rename = "totalAvailable", default)]
    pub total_available: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CncLocation {
    #[serde(default)]
    pub fulfilment: Option<CncFulfilment>,
    #[serde(default)]
    pub location: Option<LocationRef>,
    /// How far the store is from the postcode asked about. The only ordering
    /// the gateway offers, and the reason a store listing is useful without
    /// resolving every name first.
    #[serde(default, deserialize_with = "loose_f64")]
    pub distance_in_km: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationRef {
    #[serde(default)]
    pub location_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CncFulfilment {
    #[serde(default)]
    pub location_id: Option<String>,
    /// A "buddy" store fills the order for another one, so its stock is not
    /// stock you can walk in and collect at the store named.
    #[serde(default)]
    pub is_buddy_location: Option<bool>,
    #[serde(default)]
    pub stock: Option<Available>,
}

#[derive(Debug, Deserialize)]
pub struct LocationData {
    #[serde(rename = "locationQuery", default)]
    pub location: Option<LocationDetail>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationDetail {
    #[serde(default)]
    pub public_name: Option<String>,
    #[serde(default)]
    pub phone_number: Option<String>,
    #[serde(default)]
    pub address1: Option<String>,
    #[serde(default)]
    pub address2: Option<String>,
    #[serde(default)]
    pub address3: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub postcode: Option<String>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub latitude: Option<f64>,
    #[serde(default, deserialize_with = "loose_f64")]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub trading_hours: Vec<TradingHours>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradingHours {
    #[serde(default)]
    pub week_day: Option<String>,
    #[serde(default)]
    pub hours: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PostcodeData {
    #[serde(rename = "postcodeQuery", default)]
    pub suggestions: Vec<PostcodeSuggestion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostcodeSuggestion {
    #[serde(default)]
    pub postcode: Option<String>,
    /// `NI`/`SI` in New Zealand, a real state in Australia.
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub suburb: Option<String>,
    #[serde(default)]
    pub is_metro_region_for_free_shipping: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct MeData<T> {
    #[serde(default = "Option::default")]
    pub me: Option<T>,
}

#[derive(Debug, Deserialize)]
pub struct ActiveCart {
    /// `null` for a visitor. Not an error -- an anonymous cart exists only as
    /// an id a browser kept, and `me` cannot find one.
    #[serde(rename = "activeCart", default)]
    pub active_cart: Option<CartBody>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCartData {
    #[serde(rename = "updateMyCart", default)]
    pub cart: Option<CartBody>,
    #[serde(rename = "createMyCart", default)]
    pub created: Option<CartBody>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartBody {
    #[serde(default)]
    pub id: Option<String>,
    /// Optimistic concurrency: a mutation carrying a stale one is refused.
    #[serde(default)]
    pub version: Option<i64>,
    #[serde(default)]
    pub selected_cnc_store_id: Option<String>,
    #[serde(default)]
    pub total_price: Option<Money>,
    #[serde(default)]
    pub shipping_info: Option<ShippingInfo>,
    #[serde(default)]
    pub line_items: Vec<CartLineBody>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippingInfo {
    #[serde(default)]
    pub shipping_method_name: Option<String>,
    #[serde(default)]
    pub shipping_rate: Option<ShippingRate>,
}

#[derive(Debug, Deserialize)]
pub struct ShippingRate {
    #[serde(default)]
    pub price: Option<Money>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartLineBody {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub quantity: Option<i64>,
    #[serde(default)]
    pub total_price: Option<Money>,
    #[serde(default)]
    pub price: Option<PriceWrapper>,
    #[serde(default)]
    pub variant: Option<VariantBody>,
    #[serde(default)]
    pub custom: Option<CustomFields>,
}

#[derive(Debug, Deserialize)]
pub struct PriceWrapper {
    #[serde(default)]
    pub value: Option<Money>,
}

/// commercetools money: an integer minor unit, which is the one thing on this
/// wire that is already in cents.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Money {
    #[serde(default)]
    pub cent_amount: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct VariantBody {
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Deserialize)]
pub struct Attribute {
    #[serde(default)]
    pub name: Option<String>,
    /// Sometimes a string, sometimes a number, sometimes an object carrying a
    /// label -- so it stays raw and is flattened on the way out. This is the
    /// field the note about loose payloads was written for.
    #[serde(default)]
    pub value: serde_json::Value,
}

impl Attribute {
    /// The value as something printable, whatever shape it arrived in.
    pub fn text(&self) -> Option<String> {
        match &self.value {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(b.to_string()),
            // The labelled form: `{"key": "black", "label": "Black"}`.
            serde_json::Value::Object(o) => o
                .get("label")
                .or_else(|| o.get("value"))
                .or_else(|| o.get("key"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CustomFields {
    #[serde(default)]
    pub fields: Option<CartLineFields>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartLineFields {
    #[serde(default)]
    pub seller_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WishlistData {
    #[serde(rename = "defaultShoppingList", default)]
    pub list: Option<WishlistBody>,
}

#[derive(Debug, Deserialize)]
pub struct WishlistAddData {
    #[serde(rename = "addItemToShoppingList", default)]
    pub list: Option<WishlistBody>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishlistBody {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub version: Option<i64>,
    #[serde(default)]
    pub line_items: Vec<WishlistLineBody>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishlistLineBody {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub quantity: Option<i64>,
    #[serde(default)]
    pub variant: Option<WishlistVariant>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishlistVariant {
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub shopping_list_price: Option<Money>,
}

#[derive(Debug, Deserialize)]
pub struct OrdersData {
    #[serde(rename = "getOrdersForCustomer", default)]
    pub page: Option<OrderPageBody>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderPageBody {
    #[serde(default)]
    pub count: Option<i64>,
    #[serde(default)]
    pub starts_after: Option<String>,
    #[serde(default)]
    pub orders: Vec<OrderBody>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBody {
    #[serde(default)]
    pub display_order_id: Option<String>,
    /// Dollars as a number here, unlike the cart's cents. The two halves of
    /// the same gateway disagree, which is why nothing downstream sees either
    /// form.
    #[serde(default)]
    pub order_total: Option<f64>,
    #[serde(default)]
    pub order_status: Option<String>,
    #[serde(default)]
    pub order_date: Option<String>,
    #[serde(default)]
    pub shipped_items: Vec<ShippedItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippedItem {
    #[serde(default)]
    pub tracking_number: Option<String>,
    #[serde(default)]
    pub carrier: Option<String>,
    #[serde(default)]
    pub tracking_link: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CustomerData {
    #[serde(default)]
    pub customer: Option<CustomerBody>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerBody {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap()
    }

    fn parse(name: &str) -> SearchEnvelope {
        serde_json::from_str(&fixture(name)).unwrap()
    }

    #[test]
    fn a_search_page_decodes_to_products_with_prices() {
        let env = parse("search-page.json");
        assert!(env.response.total_num_results > 0);
        let first = &env.response.results[0];
        assert!(first.value.is_some());
        let d = &first.data;
        assert!(
            d.variation_id.is_some(),
            "the keycode is what everything else uses"
        );
        assert!(d.price.is_some());
        assert_eq!(d.prices[0].currency.as_deref(), Some("NZD"));
    }

    #[test]
    fn the_unstable_half_of_the_data_bag_survives_in_extra() {
        // `Capacity` and `Power Rating` exist per category. Typing them would
        // break the moment a category without them is listed.
        let env = parse("browse-group.json");
        let extra = &env.response.results[0].data.extra;
        assert!(!extra.is_empty(), "something was left over to keep");
        assert!(
            !extra.contains_key("variation_id"),
            "the typed fields are not duplicated into extra"
        );
    }

    #[test]
    fn a_keycode_is_a_search_term_that_matches_one_product() {
        let env = parse("search-keycode.json");
        assert_eq!(env.response.total_num_results, 1);
        assert_eq!(
            env.response.results[0].data.variation_id.as_deref(),
            Some("43165537")
        );
        assert!(env.response.results[0].data.description.is_some());
    }

    #[test]
    fn a_nonsense_term_still_answers_but_matches_no_token() {
        // The index falls back to embeddings, so "no results" cannot be read
        // off the count -- only off token_match.
        let env = parse("search-empty.json");
        assert!(env.response.total_num_results > 0, "it answered anyway");
        assert_eq!(
            env.response
                .result_sources
                .get("token_match")
                .map(|s| s.count),
            Some(0)
        );
    }

    #[test]
    fn variations_each_carry_their_own_keycode() {
        let env = parse("search-variations.json");
        let first = &env.response.results[0];
        assert!(first.variations.len() > 1);
        let codes: Vec<_> = first
            .variations
            .iter()
            .filter_map(|v| v.data.variation_id.as_deref())
            .collect();
        assert_eq!(
            codes.len(),
            first.variations.len(),
            "every one has a keycode"
        );
        assert!(
            codes
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 1,
            "and they differ, which is why the parent's is not enough for a cart"
        );
    }

    #[test]
    fn a_group_tree_decodes_recursively() {
        let env = parse("browse-tree.json");
        let root = &env.response.groups[0];
        assert!(!root.children.is_empty());
        let child = &root.children[0];
        assert!(child.group_id.is_some());
        assert!(child.data.url.is_some(), "the slug a person can read");
        assert!(!child.children.is_empty(), "and it nests");
    }

    #[test]
    fn a_truncated_payload_is_an_error_not_a_panic() {
        assert!(serde_json::from_str::<SearchEnvelope>("{ \"resp").is_err());
        // But an unrecognised shape decodes to nothing rather than failing.
        let env: SearchEnvelope = serde_json::from_str("{}").unwrap();
        assert!(env.response.results.is_empty());
    }
}
