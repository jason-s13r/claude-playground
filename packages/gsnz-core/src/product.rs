//! One thing on a shelf.

use serde::{Deserialize, Serialize};

use crate::money::{as_dollars_opt, dollars};
use crate::retailer::RetailerId;

/// How a line is counted. Foodstuffs calls it `SaleType`, Woolworths sends
/// `EACH`/`KILOGRAM`; both mean the same two things.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SaleUnit {
    #[default]
    Each,
    Weight,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Product {
    pub retailer: RetailerId,

    /// What a person types and what search results print.
    pub sku: String,

    /// What cart mutations are keyed on. The SKU for Foodstuffs; the variant
    /// key (`282768-EA`) for Woolworths. Opaque here -- only the adapter that
    /// produced it knows how to read it.
    pub key: String,

    pub name: String,
    pub brand: Option<String>,
    pub size: Option<String>,

    #[serde(rename = "price", serialize_with = "as_dollars_opt")]
    pub price_cents: Option<i64>,
    #[serde(rename = "was_price", serialize_with = "as_dollars_opt")]
    pub was_price_cents: Option<i64>,
    #[serde(rename = "unit_price", serialize_with = "as_dollars_opt")]
    pub unit_price_cents: Option<i64>,
    /// What the unit price is per: `1kg`, `100ml`.
    pub unit_measure: Option<String>,

    pub sale_unit: SaleUnit,
    /// Already rendered by the retailer: "2 for $5.00".
    pub multi_buy: Option<String>,

    pub is_special: bool,
    /// Foodstuffs Club, Woolworths "club price" -- one flag, both meanings.
    pub is_member_price: bool,
    pub in_stock: Option<bool>,
    pub availability: Option<String>,
    pub department: Option<String>,
    pub image: Option<String>,
    pub url: Option<String>,
}

impl Product {
    /// Brand and name, without saying the brand twice.
    ///
    /// Both retailers send `brand: "Anchor"` alongside `name: "Anchor Blue
    /// Milk"`, and joining them naively reads as "Anchor Anchor Blue Milk".
    pub fn title(&self) -> String {
        match &self.brand {
            Some(brand) if !brand.is_empty() => {
                let lower_name = self.name.to_lowercase();
                let lower_brand = brand.to_lowercase();
                if lower_name.starts_with(&lower_brand) {
                    self.name.clone()
                } else {
                    format!("{brand} {}", self.name)
                }
            }
            _ => self.name.clone(),
        }
    }

    /// How much this is down from its usual price, when both are known.
    pub fn saving_cents(&self) -> Option<i64> {
        match (self.was_price_cents, self.price_cents) {
            (Some(was), Some(now)) if was > now => Some(was - now),
            _ => None,
        }
    }

    pub fn price(&self) -> Option<String> {
        self.price_cents.map(dollars)
    }

    pub fn by_weight(&self) -> bool {
        self.sale_unit == SaleUnit::Weight
    }

    /// Client-side `--size` filter. The APIs have no size facet worth using, so
    /// this is a substring test over the size and the name.
    pub fn matches_size(&self, want: &str) -> bool {
        let want = want.trim().to_lowercase();
        if want.is_empty() {
            return true;
        }
        let in_size = self
            .size
            .as_deref()
            .is_some_and(|s| s.to_lowercase().contains(&want));
        in_size || self.name.to_lowercase().contains(&want)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(brand: Option<&str>, name: &str) -> Product {
        Product {
            retailer: RetailerId::NewWorld,
            sku: "A-EA-000".into(),
            key: "A-EA-000".into(),
            name: name.into(),
            brand: brand.map(Into::into),
            size: Some("2L".into()),
            price_cents: Some(429),
            was_price_cents: None,
            unit_price_cents: None,
            unit_measure: None,
            sale_unit: SaleUnit::Each,
            multi_buy: None,
            is_special: false,
            is_member_price: false,
            in_stock: Some(true),
            availability: None,
            department: None,
            image: None,
            url: None,
        }
    }

    #[test]
    fn does_not_repeat_a_brand_the_name_already_starts_with() {
        assert_eq!(
            product(Some("Anchor"), "Anchor Blue Milk").title(),
            "Anchor Blue Milk"
        );
        assert_eq!(
            product(Some("anchor"), "Anchor Blue Milk").title(),
            "Anchor Blue Milk"
        );
    }

    #[test]
    fn prefixes_a_brand_the_name_omits() {
        assert_eq!(
            product(Some("Anchor"), "Blue Milk").title(),
            "Anchor Blue Milk"
        );
        assert_eq!(product(None, "Blue Milk").title(), "Blue Milk");
    }

    #[test]
    fn saving_needs_a_higher_was_price() {
        let mut p = product(None, "Milk");
        assert_eq!(p.saving_cents(), None);
        p.was_price_cents = Some(500);
        assert_eq!(p.saving_cents(), Some(71));
        p.was_price_cents = Some(400); // a rise is not a saving
        assert_eq!(p.saving_cents(), None);
    }

    #[test]
    fn size_filter_looks_at_the_name_too() {
        let p = product(None, "Blue Milk 2 Litre");
        assert!(p.matches_size("2l"), "matches the size field");
        assert!(p.matches_size("2 litre"), "matches the name");
        assert!(p.matches_size(""), "an empty filter keeps everything");
        assert!(!p.matches_size("3L"));
    }
}
