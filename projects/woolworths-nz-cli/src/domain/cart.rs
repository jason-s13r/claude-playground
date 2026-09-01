//! The cart, once a `CustomerCart` response has been normalised.
//!
//! Every cart operation -- reading it, changing a quantity, emptying it --
//! answers with the same `CustomerCart` shape, so one type covers all three and
//! a mutation can render exactly the way `wwnz cart list` does.

use serde::{Deserialize, Serialize};

use crate::domain::product::title;

#[derive(Clone, Debug, Serialize)]
pub struct Cart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub lines: Vec<CartLine>,
    /// What the site says is in the cart, which is not the same as
    /// `lines.len()`: it counts quantities, not distinct products.
    pub total_items: u32,
    pub unique_products: u32,
    /// The lines alone, which is what the rows on screen add up to.
    #[serde(rename = "items", serialize_with = "crate::domain::money::as_dollars")]
    pub items_cents: Option<i64>,
    /// Products plus fees -- the site's "order subtotal". Bigger than
    /// [`Cart::items_cents`] whenever a delivery or pickup fee applies.
    #[serde(
        rename = "subtotal",
        serialize_with = "crate::domain::money::as_dollars"
    )]
    pub subtotal_cents: Option<i64>,
    #[serde(
        rename = "amount_to_pay",
        serialize_with = "crate::domain::money::as_dollars"
    )]
    pub to_pay_cents: Option<i64>,
    #[serde(
        rename = "discount",
        serialize_with = "crate::domain::money::as_dollars"
    )]
    pub discount_cents: Option<i64>,
    /// The store the cart is being fulfilled from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfilment_method: Option<String>,
    /// Anything the server refused or warned about, e.g. an out-of-stock line.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CartLine {
    pub sku: String,
    pub variant_key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    pub quantity: u32,
    #[serde(
        rename = "unit_price",
        serialize_with = "crate::domain::money::as_dollars"
    )]
    pub unit_price_cents: Option<i64>,
    #[serde(rename = "total", serialize_with = "crate::domain::money::as_dollars")]
    pub total_cents: Option<i64>,
    /// What this line saves against the undiscounted price.
    #[serde(
        rename = "discount",
        serialize_with = "crate::domain::money::as_dollars"
    )]
    pub discount_cents: Option<i64>,
    pub can_substitute: bool,
}

impl CartLine {
    pub fn title(&self) -> String {
        title(self.brand.as_deref(), &self.name)
    }
}

impl Cart {
    /// What the fee-inclusive subtotal adds on top of the lines, when the two
    /// differ. `None` when there is nothing to explain.
    pub fn fees_cents(&self) -> Option<i64> {
        let (subtotal, items) = (self.subtotal_cents?, self.items_cents?);
        (subtotal > items).then_some(subtotal - items)
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn line(&self, sku_or_key: &str) -> Option<&CartLine> {
        let needle = sku_or_key.trim();
        self.lines.iter().find(|l| {
            l.sku.eq_ignore_ascii_case(needle) || l.variant_key.eq_ignore_ascii_case(needle)
        })
    }
}

/// One quantity change to apply. A quantity of zero removes the line.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    #[serde(rename = "variantKey")]
    pub variant_key: String,
    pub quantity: u32,
}

/// The variant key a cart mutation needs, from whatever the user typed.
///
/// `wwnz search` prints both the SKU and the variant key, and people type the
/// SKU. A bare stock code is completed to the `-EA` variant, which is what all
/// but the weighed items are; anything already carrying a unit suffix is left
/// alone.
pub fn variant_key(sku_or_key: &str, unit: Option<&str>) -> String {
    let raw = sku_or_key.trim();
    if let Some(unit) = unit.map(str::trim).filter(|u| !u.is_empty()) {
        let stock_code = raw.split('-').next().unwrap_or(raw);
        return format!("{stock_code}-{}", unit.to_uppercase());
    }
    if raw.contains('-') {
        return raw.to_string();
    }
    format!("{raw}-EA")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_stock_code_becomes_the_each_variant() {
        assert_eq!(variant_key("282768", None), "282768-EA");
        assert_eq!(variant_key("  282768 ", None), "282768-EA");
    }

    #[test]
    fn a_key_that_already_names_its_unit_is_left_alone() {
        assert_eq!(variant_key("282768-EA", None), "282768-EA");
        assert_eq!(variant_key("123456-KGM", None), "123456-KGM");
    }

    #[test]
    fn an_explicit_unit_replaces_whatever_was_there() {
        assert_eq!(variant_key("282768", Some("kgm")), "282768-KGM");
        assert_eq!(variant_key("282768-EA", Some("KGM")), "282768-KGM");
        // A blank unit is not a choice.
        assert_eq!(variant_key("282768-EA", Some("  ")), "282768-EA");
    }

    #[test]
    fn a_line_is_found_by_either_of_its_two_names() {
        let cart = Cart {
            id: None,
            lines: vec![CartLine {
                sku: "282768".into(),
                variant_key: "282768-EA".into(),
                name: "Milk Standard 3L".into(),
                brand: Some("Woolworths".into()),
                quantity: 1,
                unit_price_cents: Some(719),
                total_cents: Some(719),
                discount_cents: None,
                can_substitute: false,
            }],
            total_items: 1,
            unique_products: 1,
            items_cents: Some(719),
            subtotal_cents: Some(719),
            to_pay_cents: Some(719),
            discount_cents: None,
            store_name: None,
            store_id: None,
            fulfilment_method: None,
            problems: Vec::new(),
        };
        assert!(cart.line("282768").is_some());
        assert!(cart.line("282768-EA").is_some());
        assert!(cart.line("999999").is_none());
        assert!(!cart.is_empty());
    }
}
