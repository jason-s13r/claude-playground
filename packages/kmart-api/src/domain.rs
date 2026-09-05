//! The types this crate hands back.
//!
//! Vendor-shaped and final: nothing above this converts them into a shared
//! domain, because there is no second retailer here to share one with.
//!
//! Money is in **cents**, always. Constructor states a price three ways in the
//! same record -- a number in dollars, a decimal string, and sometimes neither
//! -- and every one of those is a rounding bug waiting to be printed. One
//! integer at the boundary means the conversion happens once, here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::country::Country;

/// An amount, in cents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Price {
    pub cents: i64,
}

impl Price {
    pub fn cents(cents: i64) -> Price {
        Price { cents }
    }

    /// From Constructor's dollars-as-a-float.
    ///
    /// Rounded rather than truncated: `39.99_f64` is stored as slightly less
    /// than 39.99, and truncating that yields 3998.
    pub fn from_dollars(dollars: f64) -> Price {
        Price {
            cents: (dollars * 100.0).round() as i64,
        }
    }

    /// From Constructor's decimal string, `"39.00"`.
    ///
    /// Parsed as a decimal rather than through `f64` so that the string form,
    /// which is the exact one, does not take a detour through binary floating
    /// point.
    pub fn parse(text: &str) -> Option<Price> {
        let text = text.trim();
        let (sign, text) = match text.strip_prefix('-') {
            Some(rest) => (-1, rest),
            None => (1, text),
        };
        let (whole, frac) = match text.split_once('.') {
            Some((w, f)) => (w, f),
            None => (text, ""),
        };
        if whole.is_empty() && frac.is_empty() {
            return None;
        }
        let whole: i64 = if whole.is_empty() {
            0
        } else {
            whole.parse().ok()?
        };
        // Two decimal places, padded or truncated. A feed that starts quoting
        // four is not a reason to fail.
        let cents: i64 = match frac.len() {
            0 => 0,
            1 => format!("{frac}0").parse().ok()?,
            _ => frac[..2].parse().ok()?,
        };
        Some(Price {
            cents: sign * (whole * 100 + cents),
        })
    }

    pub fn dollars(self) -> f64 {
        self.cents as f64 / 100.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rating {
    pub score: f64,
    pub reviews: u64,
}

/// One product, as the catalogue knows it.
///
/// Everything except the keycode is optional, deliberately: the index is fed
/// per category and a t-shirt has no `Capacity` while an appliance has no
/// `Size Range`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Product {
    /// The one id that works everywhere: Constructor's `variation_id`, the
    /// gateway's `sku`, and the tail of the page slug.
    pub keycode: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Price>,
    /// The higher price a markdown was taken from, when the record carries
    /// one. Only ever set when it exceeds [`Product::price`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub was: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Who sells it. `Kmart` for Kmart's own stock, a business name for a
    /// marketplace listing -- which ships separately and is not in any store.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seller: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<Rating>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// The other shots, as bare asset paths rather than URLs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// HTML, as the feed carries it. Not printable as-is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub free_shipping: bool,
    /// Stock is pooled nationally rather than per store. The availability
    /// query needs this and can get it nowhere else.
    pub national_inventory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<String>,
    /// Sizes and colours, each with a keycode of its own. Empty when the
    /// product has only the one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variations: Vec<Variation>,
    /// Whatever else the feed carried for this product.
    ///
    /// Per category and unstable -- `Capacity` on a bucket, `Power Rating` on
    /// an appliance, `Book Genre` on a novel -- so it is passed through as
    /// raw JSON rather than typed. A caller printing `--json` gets it; nothing
    /// here reads it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Product {
    /// Whether a marketplace seller rather than Kmart is behind it.
    pub fn marketplace(&self) -> bool {
        self.seller
            .as_deref()
            .is_some_and(|s| !s.eq_ignore_ascii_case("kmart"))
    }

    /// The full storefront URL, for a country.
    pub fn page(&self, country: Country) -> Option<String> {
        let path = self.url.as_deref()?;
        Some(format!("{}{}", country.origin(), path))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Variation {
    pub keycode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Price>,
}

/// A page of products, and what the index said about the query behind it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Listing {
    pub products: Vec<Product>,
    pub total: u64,
    /// How many results actually matched the words typed.
    ///
    /// Worth carrying separately from `total`: the index falls back to a
    /// semantic match, so a term matching nothing still answers with twenty
    /// products. Without this a caller cannot tell a real result from a polite
    /// guess.
    pub exact: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facets: Vec<Facet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sorts: Vec<Sort>,
}

impl Listing {
    /// Whether the results are a fallback rather than a match.
    pub fn is_guess(&self) -> bool {
        self.exact == Some(0) && !self.products.is_empty()
    }
}

/// A refinement the index offers, with the values it has. Per category, which
/// is why a caller discovers them rather than being given a fixed list.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Facet {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub options: Vec<FacetOption>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FacetOption {
    pub value: String,
    /// How the site spells it, when that differs from the value sent back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    pub by: String,
    pub order: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A category. `children` is populated only as deep as the request asked for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
    /// The 32-hex group id, which is what a listing is fetched by.
    pub id: String,
    pub name: String,
    /// The storefront path. The only handle on a category a person can read,
    /// and what `browse` matches a typed name against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Category>,
}

impl Category {
    /// Depth-first, self first. For matching a typed name against the tree.
    pub fn walk(&self) -> Vec<&Category> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.walk());
        }
        out
    }
}

// --------------------------------------------------------------- the gateway

/// A postcode, and what the gateway knows about it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Postcode {
    pub postcode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suburb: Option<String>,
    /// `NI`/`SI` in New Zealand, a state in Australia. The New Zealand form is
    /// also what the catalogue's island filter wants, which is the one place
    /// the two halves of this crate meet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metro: Option<bool>,
}

/// A shop.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Store {
    /// The `locationId`. One namespace across both countries -- asking the
    /// Australian gateway about `8229` describes a shop in Auckland.
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub address: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postcode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    /// Kilometres from the postcode that was asked about, when the answer came
    /// from an availability query. Absent when the store was fetched by id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hours: Vec<TradingDay>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradingDay {
    pub day: String,
    pub hours: String,
}

/// How much of something one store has.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreStock {
    pub store_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub available: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
    /// A buddy store fills orders on another store's behalf, so its stock is
    /// not necessarily on a shelf you can walk to.
    pub buddy: bool,
}

/// Where a product can be had, near a postcode.
///
/// Every channel is independent and any of them can be absent: a marketplace
/// item ships and is in no store, a bulky item is collect-only, and a product
/// Kmart has stopped ranging answers zero on all of them rather than 404ing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Availability {
    pub keycode: String,
    pub postcode: String,
    /// `METRO` and the like. What the free-shipping threshold keys off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Pooled stock for delivery, and the fulfilment pool's own name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_delivery: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub express: Option<i64>,
    /// Total across the collect network, which is more than the stores listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_and_collect: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stores: Vec<StoreStock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub in_store: Vec<StoreStock>,
}

impl Availability {
    /// Whether it can be had at all, by any route.
    pub fn any(&self) -> bool {
        self.home_delivery.unwrap_or(0) > 0
            || self.express.unwrap_or(0) > 0
            || self.click_and_collect.unwrap_or(0) > 0
            || self.stores.iter().any(|s| s.available > 0)
            || self.in_store.iter().any(|s| s.available > 0)
    }
}

/// The shopping bag.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cart {
    pub id: String,
    /// Optimistic concurrency. Every write carries the version it read, and
    /// the gateway refuses one that has moved on -- so this is not decoration,
    /// it is the thing that makes a change safe.
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collect_store_id: Option<String>,
    /// Delivery, which `total` already includes.
    ///
    /// Carried separately because otherwise the total does not appear to add
    /// up: a seven dollar item in a thirteen dollar cart reads as a bug.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shipping: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shipping_method: Option<String>,
    pub lines: Vec<CartLine>,
}

impl Cart {
    /// The line holding a keycode, which is what a quantity change needs --
    /// the gateway addresses a line by its own id, not by the product's.
    pub fn line(&self, keycode: &str) -> Option<&CartLine> {
        self.lines.iter().find(|l| l.keycode == keycode)
    }

    pub fn items(&self) -> i64 {
        self.lines.iter().map(|l| l.quantity).sum()
    }

    /// What the goods come to, before delivery.
    pub fn subtotal(&self) -> Price {
        Price::cents(
            self.lines
                .iter()
                .filter_map(|l| l.total)
                .map(|p| p.cents)
                .sum(),
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CartLine {
    /// The line's id, which is not the product's.
    pub id: String,
    pub keycode: String,
    pub name: String,
    pub quantity: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_price: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seller: Option<String>,
    /// The variant's size and colour, as `(name, value)`.
    ///
    /// Only those two. The gateway returns the whole merchandising record on
    /// a line item -- tax rates, dimensions, Bynder image keys, forty fields
    /// of it -- and printing that is worse than printing nothing. These are
    /// the two that tell one variant from another, which is the only reason
    /// this exists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<(String, String)>,
}

/// What is saved for later.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wishlist {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    pub items: Vec<WishlistItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WishlistItem {
    pub id: String,
    pub keycode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub quantity: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Price>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placed: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracking: Vec<Tracking>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tracking {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carrier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// A page of orders. `next` is the cursor for the following one.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderPage {
    pub orders: Vec<Order>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// Who is signed in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Customer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decimal_price_string_does_not_go_through_a_float() {
        assert_eq!(Price::parse("39.00"), Some(Price::cents(3900)));
        assert_eq!(Price::parse("0.05"), Some(Price::cents(5)));
        assert_eq!(Price::parse("12"), Some(Price::cents(1200)));
        assert_eq!(Price::parse("12.5"), Some(Price::cents(1250)));
        assert_eq!(Price::parse("-3.50"), Some(Price::cents(-350)));
        assert_eq!(Price::parse("what"), None);
    }

    #[test]
    fn dollars_as_a_float_are_rounded_not_truncated() {
        // 39.99 is not representable, and truncating it gives 3998.
        assert_eq!(Price::from_dollars(39.99), Price::cents(3999));
        assert_eq!(Price::from_dollars(39.0), Price::cents(3900));
    }

    fn product(seller: Option<&str>) -> Product {
        Product {
            keycode: "43165537".into(),
            name: "Milk Frother".into(),
            brand: None,
            price: None,
            was: None,
            currency: None,
            colour: None,
            size: None,
            seller: seller.map(str::to_string),
            rating: None,
            url: Some("/product/milk-frother-black-43165537/".into()),
            image: None,
            description: None,
            free_shipping: false,
            national_inventory: false,
            category_id: None,
            variations: Vec::new(),
            images: Vec::new(),
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn a_marketplace_seller_is_told_apart_from_kmart() {
        // It matters: a marketplace item ships separately and is in no store,
        // so `stock` has nothing to say about it.
        assert!(!product(Some("Kmart")).marketplace());
        assert!(!product(Some("kmart")).marketplace());
        assert!(product(Some("Some Other Seller")).marketplace());
        assert!(!product(None).marketplace());
    }

    #[test]
    fn a_product_page_url_follows_the_country() {
        let p = product(None);
        assert_eq!(
            p.page(Country::Nz).unwrap(),
            "https://www.kmart.co.nz/product/milk-frother-black-43165537/"
        );
        assert_eq!(
            p.page(Country::Au).unwrap(),
            "https://www.kmart.com.au/product/milk-frother-black-43165537/"
        );
    }

    #[test]
    fn a_listing_knows_when_its_results_are_a_guess() {
        let guess = Listing {
            products: vec![product(None)],
            total: 20,
            exact: Some(0),
            facets: Vec::new(),
            sorts: Vec::new(),
        };
        assert!(guess.is_guess());

        let real = Listing {
            exact: Some(20),
            ..guess.clone()
        };
        assert!(!real.is_guess());

        // Nothing at all is not a guess either -- there is nothing to warn about.
        let none = Listing {
            products: Vec::new(),
            exact: Some(0),
            ..real.clone()
        };
        assert!(!none.is_guess());
    }

    fn stock(id: &str, available: i64) -> StoreStock {
        StoreStock {
            store_id: id.into(),
            name: None,
            available,
            distance_km: None,
            buddy: false,
        }
    }

    #[test]
    fn availability_is_per_channel_and_any_one_of_them_counts() {
        let mut a = Availability {
            keycode: "43165537".into(),
            postcode: "1010".into(),
            region: None,
            home_delivery: Some(0),
            pool: None,
            express: None,
            click_and_collect: Some(0),
            stores: Vec::new(),
            in_store: Vec::new(),
        };
        assert!(!a.any(), "zero everywhere");

        // Sold out online but on a shelf is a real and common state, and the
        // whole reason this is not a boolean.
        a.stores = vec![stock("8229", 4)];
        assert!(a.any());
    }

    #[test]
    fn a_cart_line_is_addressed_by_its_own_id_not_the_products() {
        let cart = Cart {
            id: "c".into(),
            version: 3,
            // Postage on top, as the gateway reports it.
            total: Some(Price::cents(8400)),
            collect_store_id: None,
            shipping: Some(Price::cents(600)),
            shipping_method: Some("Standard".into()),
            lines: vec![
                CartLine {
                    id: "line-1".into(),
                    keycode: "43165537".into(),
                    name: "Milk Frother".into(),
                    quantity: 2,
                    unit_price: Some(Price::cents(3900)),
                    total: Some(Price::cents(7800)),
                    seller: None,
                    options: Vec::new(),
                },
                CartLine {
                    id: "line-2".into(),
                    keycode: "43226382".into(),
                    name: "Mop".into(),
                    quantity: 1,
                    unit_price: None,
                    total: None,
                    seller: None,
                    options: Vec::new(),
                },
            ],
        };
        assert_eq!(cart.line("43165537").map(|l| l.id.as_str()), Some("line-1"));
        assert!(cart.line("nope").is_none());
        assert_eq!(cart.items(), 3, "quantities, not lines");
        // The goods alone. `total` includes delivery, so without this the
        // figures look like they do not add up.
        assert_eq!(cart.subtotal(), Price::cents(7800));
    }

    #[test]
    fn walking_a_tree_visits_every_node_once() {
        let leaf = |id: &str| Category {
            id: id.into(),
            name: id.into(),
            path: None,
            count: None,
            children: Vec::new(),
        };
        let tree = Category {
            children: vec![
                Category {
                    children: vec![leaf("c")],
                    ..leaf("b")
                },
                leaf("d"),
            ],
            ..leaf("a")
        };
        let ids: Vec<&str> = tree.walk().iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c", "d"]);
    }
}
