//! What this crate hands back.
//!
//! Vendor-shaped: these are The Warehouse's own ideas, named the way the
//! storefront names them, not mapped onto a shared retail vocabulary. The
//! general-merchandise catalogue is the reason -- a grocery domain has no place
//! to put a colour axis, and this one has no place to put a price per kilo.
//!
//! Everything optional. The listing half is scraped from HTML and the JSON half
//! is undocumented, so a field The Warehouse renames should degrade to a
//! missing column rather than a failed command.

use serde::{Deserialize, Serialize};

/// An amount as the site quotes it.
///
/// Both halves are kept: `value` is what sorting and arithmetic need,
/// `formatted` is what the site chose to display and is the honest thing to
/// print. A listing tile carries only the formatted string, so `value` is
/// parsed back out of it and can be absent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub value: Option<f64>,
    pub formatted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

impl Price {
    pub fn is_empty(&self) -> bool {
        self.value.is_none() && self.formatted.is_none()
    }

    /// `$12.00`, or the number formatted the same way when only that is known.
    pub fn label(&self) -> Option<String> {
        self.formatted
            .clone()
            .or_else(|| self.value.map(|v| format!("${v:.2}")))
    }

    /// Parse a displayed price back to a number, so a scraped tile can still be
    /// sorted. Anything unparseable leaves `value` empty rather than failing.
    pub fn from_display(text: &str) -> Price {
        let cleaned: String = text
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        Price {
            value: cleaned.parse().ok(),
            formatted: Some(text.trim().to_string()),
            currency: None,
        }
    }
}

/// Whether a thing can be had, and by which route.
///
/// Two axes rather than a boolean, because this retailer genuinely has
/// four states: an item can be orderable online, orderable only by walking into
/// a shop, both, or neither. Collapsing that to "out of stock" would print
/// exactly the wrong thing for a shelf full of stock.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Availability {
    /// The site's own word: `IN_STOCK`, `FIND_IN_STORE`, `OUT_OF_STOCK`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Orderable for delivery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online: Option<bool>,
    /// Orderable through a store -- click and collect, or on the shelf.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_store: Option<bool>,
    /// What the site would print, e.g. `In-store only`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Availability {
    /// Whether it can be had at all, by any route. `None` when nothing said.
    pub fn orderable(&self) -> Option<bool> {
        match (self.online, self.in_store) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(false) || b.unwrap_or(false)),
        }
    }

    /// One short word for a table cell.
    pub fn summary(&self) -> &'static str {
        match (self.online, self.in_store) {
            (Some(true), _) => "in stock",
            (_, Some(true)) => "in store",
            (Some(false), Some(false)) | (Some(false), None) | (None, Some(false)) => "sold out",
            (None, None) => "-",
        }
    }
}

/// A product as a listing or a detail page describes it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Product {
    /// The id that addresses this product -- what a URL and the cart take.
    ///
    /// Not always the same as [`Product::master_id`]: a listing tile links to a
    /// variation group (`RM110164727-1M`) while its tracking payload names the
    /// master (`RM110164727`). This is the one that works.
    pub id: String,
    /// The variation master, when this is one of its members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ean: Option<String>,
    pub price: Price,
    /// The crossed-out price, when the item is reduced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub was_price: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    /// The primary category path, slash-separated as the site writes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub availability: Availability,
    /// Sold by a third party through The Warehouse, with its own shipping.
    pub marketplace: bool,
}

impl Product {
    /// Whether this is one member of a variation family, so a caller knows the
    /// price and stock shown are for one colour rather than all of them.
    pub fn is_variant(&self) -> bool {
        self.master_id.as_deref().is_some_and(|m| m != self.id)
    }
}

/// One axis a product varies along -- colour, size.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariationAxis {
    pub id: String,
    pub name: String,
    /// What is currently chosen, when anything is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
    pub values: Vec<VariationValue>,
}

/// One choice on an axis.
///
/// `selectable` and `orderable` are different questions and the site answers
/// both: a size can be a real combination with the chosen colour (selectable)
/// and still be sold out (not orderable).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariationValue {
    pub id: String,
    pub label: String,
    pub selected: bool,
    pub selectable: bool,
    pub orderable: bool,
    /// The pre-signed URL that selects this value. Carries a `verify` token, so
    /// it is only good for as long as the page it came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A product detail, which is a [`Product`] plus what only a detail page knows.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductDetail {
    #[serde(flatten)]
    pub product: Product,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_quantity: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub axes: Vec<VariationAxis>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shipping: Vec<ShippingOption>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShippingOption {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<String>,
    pub pickup: bool,
}

/// A shop.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postcode: Option<String>,
    /// The ISO region code, `NZ-AUK` and the like. Also what the store finder
    /// is queried by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_and_collect: Option<bool>,
    /// Today's hours as the site phrases them, e.g. `8.00am - 9.00pm`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hours_today: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_now: Option<bool>,
}

/// What one store holds of one product.
///
/// The stock endpoint answers with rendered markup rather than a level, so
/// `label` is the site's own words and `in_stock` is what could be read out of
/// them. When the phrasing changes, `in_stock` goes empty and the label still
/// prints.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreStock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
    pub store_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock: Option<bool>,
}

/// The basket.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub lines: Vec<CartLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtotal: Option<String>,
    /// Total units, which is not the number of lines.
    pub quantity: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CartLine {
    /// The line's own id.
    pub uuid: String,
    /// The *other* line id, which is what removal takes.
    ///
    /// The site reports two per line -- `uuid` and `preOrderUUID`, the latter
    /// echoed as `UUID` -- and they are different values.
    /// `Cart-RemoveProductLineItem` accepts only this one and refuses the
    /// other with a generic "unable to remove".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pli_uuid: Option<String>,
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    pub quantity: u32,
    /// What one of them costs.
    pub price: Price,
    /// What the line costs. Kept apart from [`CartLine::price`] because the
    /// two controllers disagree about which they send -- `Cart-AddProduct`
    /// gives the unit price and `Cart-MiniCartShow` gives the line total -- and
    /// one field holding whichever arrived means a column headed "Price" that
    /// silently changes meaning between commands.
    pub total: Price,
}

/// A node of the department tree.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
    /// The id, which is what `cgid` takes when browsing.
    pub id: String,
    pub name: String,
    /// The `/c/...` path, which is what a landing page is fetched by. Not
    /// derivable from the id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Category>,
}

impl Category {
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Category::count).sum::<usize>()
    }

    /// Depth-first search by id or by name, case-insensitively.
    pub fn find(&self, needle: &str) -> Option<&Category> {
        if self.id.eq_ignore_ascii_case(needle) || self.name.eq_ignore_ascii_case(needle) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(needle))
    }
}

/// Which island stock is quoted for.
///
/// The Warehouse ranges differently north and south, so this changes what a
/// listing contains -- it is not a display preference. It rides on every
/// listing request as an `islandAvailability` refinement.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Island {
    North,
    South,
}

impl Island {
    /// The refinement value the site uses.
    pub fn value(self) -> &'static str {
        match self {
            Island::North => "northIsland",
            Island::South => "southIsland",
        }
    }

    pub fn parse(text: &str) -> Option<Island> {
        match text
            .trim()
            .to_lowercase()
            .replace([' ', '-', '_'], "")
            .as_str()
        {
            "north" | "northisland" | "n" | "ni" => Some(Island::North),
            "south" | "southisland" | "s" | "si" => Some(Island::South),
            _ => None,
        }
    }
}

impl std::fmt::Display for Island {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Island::North => "north",
            Island::South => "south",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_displayed_price_is_parsed_back_but_keeps_what_was_shown() {
        let p = Price::from_display("  $12.00 ");
        assert_eq!(p.value, Some(12.0));
        assert_eq!(p.formatted.as_deref(), Some("$12.00"));
        assert_eq!(p.label().as_deref(), Some("$12.00"));
    }

    #[test]
    fn an_unparseable_price_still_prints_what_the_site_said() {
        let p = Price::from_display("See in store");
        assert_eq!(p.value, None);
        assert_eq!(p.label().as_deref(), Some("See in store"));
        assert!(!p.is_empty());
    }

    #[test]
    fn in_store_only_stock_is_not_sold_out() {
        // The case that made two axes necessary: online says no, the shelf says
        // yes, and printing "sold out" would be wrong.
        let a = Availability {
            status: Some("FIND_IN_STORE".into()),
            online: Some(false),
            in_store: Some(true),
            label: Some("In-store only".into()),
        };
        assert_eq!(a.orderable(), Some(true));
        assert_eq!(a.summary(), "in store");
    }

    #[test]
    fn nothing_known_is_not_reported_as_sold_out() {
        assert_eq!(Availability::default().orderable(), None);
        assert_eq!(Availability::default().summary(), "-");
    }

    #[test]
    fn a_variation_group_is_told_apart_from_its_master() {
        let p = Product {
            id: "RM110164727-1M".into(),
            master_id: Some("RM110164727".into()),
            ..Product::default()
        };
        assert!(p.is_variant());

        let plain = Product {
            id: "R3059518".into(),
            master_id: Some("R3059518".into()),
            ..Product::default()
        };
        assert!(
            !plain.is_variant(),
            "a master that is its own id is not a variant"
        );
    }

    #[test]
    fn an_island_is_named_however_it_is_typed() {
        assert_eq!(Island::parse("North Island"), Some(Island::North));
        assert_eq!(Island::parse("si"), Some(Island::South));
        assert_eq!(Island::parse("chatham"), None);
        assert_eq!(Island::North.value(), "northIsland");
    }

    #[test]
    fn a_category_is_searchable_by_id_or_by_name() {
        let tree = Category {
            id: "root".into(),
            name: "Root".into(),
            path: None,
            children: vec![Category {
                id: "toysbaby".into(),
                name: "Toys & Baby".into(),
                path: Some("/c/toys-baby".into()),
                children: vec![],
            }],
        };
        assert_eq!(tree.count(), 2);
        assert_eq!(
            tree.find("TOYSBABY").map(|c| c.id.as_str()),
            Some("toysbaby")
        );
        assert_eq!(
            tree.find("toys & baby").map(|c| c.id.as_str()),
            Some("toysbaby")
        );
        assert!(tree.find("groceries").is_none());
    }
}
