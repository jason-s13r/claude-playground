//! The basket, and the one type that travels back to a retailer.

use serde::{Deserialize, Serialize};

use crate::money::{as_dollars, as_dollars_opt};
use crate::product::SaleUnit;
use crate::retailer::RetailerId;
use crate::store::StoreRef;

/// How much of something.
///
/// Foodstuffs sends an integer plus a sale type; Woolworths sends one float and
/// infers the rest from the variant key. Neither shape is right for the other,
/// so this is the union and the adapters convert. A weight line is kilograms
/// here regardless of what the wire wants -- Foodstuffs takes grams.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "lowercase")]
pub enum Quantity {
    Units { count: u32 },
    Kilograms { kg: f64 },
}

impl Quantity {
    pub fn units(count: u32) -> Quantity {
        Quantity::Units { count }
    }

    pub fn kilograms(kg: f64) -> Quantity {
        Quantity::Kilograms { kg }
    }

    /// Zero means remove the line, at both retailers.
    pub fn is_zero(&self) -> bool {
        match self {
            Quantity::Units { count } => *count == 0,
            Quantity::Kilograms { kg } => *kg <= 0.0,
        }
    }

    pub fn sale_unit(&self) -> SaleUnit {
        match self {
            Quantity::Units { .. } => SaleUnit::Each,
            Quantity::Kilograms { .. } => SaleUnit::Weight,
        }
    }

    /// `2` and `1.5kg` -- a whole number of units should not print as `2.0`.
    pub fn format(&self) -> String {
        match self {
            Quantity::Units { count } => count.to_string(),
            Quantity::Kilograms { kg } => {
                let trimmed = format!("{kg:.3}");
                let trimmed = trimmed.trim_end_matches('0').trim_end_matches('.');
                format!("{trimmed}kg")
            }
        }
    }
}

/// Money in a cart that is neither the subtotal nor the total.
///
/// A list rather than named fields on purpose: Foodstuffs has a service fee, a
/// bag fee, a promo discount and a subscription discount, Woolworths has one
/// discount, and the next fee either invents becomes a new row here instead of
/// a number that quietly stops being displayed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Adjustment {
    pub label: String,
    #[serde(serialize_with = "as_dollars")]
    pub cents: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CartLine {
    pub key: String,
    pub sku: String,
    pub name: String,
    pub brand: Option<String>,
    pub quantity: Quantity,
    #[serde(rename = "unit_price", serialize_with = "as_dollars_opt")]
    pub unit_price_cents: Option<i64>,
    #[serde(rename = "total", serialize_with = "as_dollars_opt")]
    pub total_cents: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cart {
    pub retailer: RetailerId,
    pub store: Option<StoreRef>,
    pub lines: Vec<CartLine>,
    /// Lines the store cannot currently supply. Foodstuffs reports these
    /// separately; leaving them in `lines` would make the totals lie.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unavailable: Vec<CartLine>,
    #[serde(rename = "subtotal", serialize_with = "as_dollars_opt")]
    pub subtotal_cents: Option<i64>,
    #[serde(rename = "total", serialize_with = "as_dollars_opt")]
    pub total_cents: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub adjustments: Vec<Adjustment>,
    pub member: Option<bool>,
    pub fulfilment: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    pub priced_at: Option<String>,
}

impl Cart {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.unavailable.is_empty()
    }

    /// Find a line by whichever identifier the user typed.
    pub fn line(&self, key_or_sku: &str) -> Option<&CartLine> {
        let want = key_or_sku.to_lowercase();
        self.lines
            .iter()
            .find(|l| l.key.to_lowercase() == want || l.sku.to_lowercase() == want)
    }
}

/// A requested change. Quantity zero removes the line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub key: String,
    pub quantity: Quantity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_units_do_not_print_a_decimal_point() {
        assert_eq!(Quantity::units(2).format(), "2");
        assert_eq!(Quantity::kilograms(1.5).format(), "1.5kg");
        assert_eq!(Quantity::kilograms(2.0).format(), "2kg");
        assert_eq!(Quantity::kilograms(0.25).format(), "0.25kg");
    }

    #[test]
    fn zero_means_remove_in_both_shapes() {
        assert!(Quantity::units(0).is_zero());
        assert!(Quantity::kilograms(0.0).is_zero());
        assert!(!Quantity::units(1).is_zero());
        assert!(!Quantity::kilograms(0.1).is_zero());
    }

    #[test]
    fn a_quantity_carries_its_sale_unit() {
        assert_eq!(Quantity::units(1).sale_unit(), SaleUnit::Each);
        assert_eq!(Quantity::kilograms(1.0).sale_unit(), SaleUnit::Weight);
    }

    #[test]
    fn quantity_round_trips_through_json() {
        for q in [Quantity::units(3), Quantity::kilograms(1.25)] {
            let s = serde_json::to_string(&q).unwrap();
            assert_eq!(serde_json::from_str::<Quantity>(&s).unwrap(), q, "{s}");
        }
    }
}
