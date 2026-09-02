//! A product, once a search response has been normalised.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Product {
    /// The stock code, which is what `wwnz cart add` takes and what the image
    /// CDN is addressed by.
    pub sku: String,
    /// The key a cart line is actually keyed on: `<sku>-<unit>`, e.g.
    /// `282768-EA`. The cart mutations want this rather than the bare SKU.
    pub variant_key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    /// What one unit is: `EACH` for counted items, `KILOGRAM` for weighed ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_of_measure: Option<String>,
    #[serde(rename = "price", serialize_with = "crate::domain::money::as_dollars")]
    pub price_cents: Option<i64>,
    /// The price before a special, when there is one to strike through.
    #[serde(
        rename = "was_price",
        serialize_with = "crate::domain::money::as_dollars"
    )]
    pub was_price_cents: Option<i64>,
    #[serde(
        rename = "unit_price",
        serialize_with = "crate::domain::money::as_dollars"
    )]
    pub unit_price_cents: Option<i64>,
    /// What the unit price is per: "1L", "100g".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_measure: Option<String>,
    pub is_special: bool,
    /// A price only members get, which the site badges separately from a
    /// special.
    pub is_club_price: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    /// Where the price came from. Prices are per store, so a result that was
    /// priced somewhere unexpected is worth being able to see.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_key: Option<String>,
    /// Whether the site marked this as an ad rather than an organic result.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sponsored: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub url: String,
}

/// Brand and name with the brand stripped off the front of the name, which the
/// catalogue repeats constantly ("Anchor Anchor Blue Milk 2L"). Cart lines
/// carry the two apart in the same way, so they share this.
pub fn title(brand: Option<&str>, name: &str) -> String {
    let brand = brand.unwrap_or("").trim();
    if brand.is_empty() {
        return name.trim().to_string();
    }
    let name = name.trim();
    let rest = if name.to_lowercase().starts_with(&brand.to_lowercase()) {
        name[brand.len()..].trim()
    } else {
        name
    };
    format!("{brand} {rest}").trim().to_string()
}

impl Product {
    pub fn title(&self) -> String {
        title(self.brand.as_deref(), &self.name)
    }

    /// How much a special saves, when both prices are known.
    pub fn saving_cents(&self) -> Option<i64> {
        let (was, now) = (self.was_price_cents?, self.price_cents?);
        (was > now).then_some(was - now)
    }

    /// Whether this is sold by weight, which changes what a cart quantity
    /// means: grams rather than a count.
    pub fn by_weight(&self) -> bool {
        self.unit_of_measure
            .as_deref()
            .is_some_and(|u| u.eq_ignore_ascii_case("KILOGRAM") || u.eq_ignore_ascii_case("KG"))
    }

    pub fn matches_size(&self, needle: &str) -> bool {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        self.name.to_lowercase().contains(&needle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(brand: Option<&str>, name: &str) -> Product {
        Product {
            sku: "282768".into(),
            variant_key: "282768-EA".into(),
            name: name.into(),
            brand: brand.map(str::to_string),
            unit_of_measure: Some("EACH".into()),
            price_cents: Some(719),
            was_price_cents: None,
            unit_price_cents: Some(240),
            unit_measure: Some("1L".into()),
            is_special: false,
            is_club_price: false,
            in_stock: Some(true),
            availability: Some("IN_STOCK".into()),
            department: None,
            store_key: Some("9171".into()),
            sponsored: false,
            image: None,
            url: String::new(),
        }
    }

    #[test]
    fn title_does_not_repeat_the_brand() {
        assert_eq!(
            product(Some("Woolworths"), "Woolworths Milk Standard 3L").title(),
            "Woolworths Milk Standard 3L"
        );
        assert_eq!(
            product(Some("Anchor"), "Milk Standard Blue 1L").title(),
            "Anchor Milk Standard Blue 1L"
        );
        assert_eq!(product(None, "Loose Bananas").title(), "Loose Bananas");
    }

    #[test]
    fn a_saving_needs_both_prices_and_a_real_drop() {
        let mut p = product(None, "x");
        assert_eq!(p.saving_cents(), None, "no was-price means no saving");
        p.was_price_cents = Some(899);
        assert_eq!(p.saving_cents(), Some(180));
        // A "was" price at or below the current one is not a saving.
        p.was_price_cents = Some(719);
        assert_eq!(p.saving_cents(), None);
    }

    #[test]
    fn weighed_items_are_recognised_from_their_unit() {
        assert!(!product(None, "x").by_weight());
        let mut p = product(None, "Beef Mince");
        p.unit_of_measure = Some("KILOGRAM".into());
        assert!(p.by_weight());
    }

    #[test]
    fn size_filtering_looks_at_the_name() {
        let p = product(Some("Woolworths"), "Woolworths Milk Standard 3L");
        assert!(p.matches_size("3l"));
        assert!(p.matches_size(""));
        assert!(!p.matches_size("600ml"));
    }

    #[test]
    fn json_prices_come_out_in_dollars() {
        let json = serde_json::to_value(product(None, "x")).unwrap();
        assert_eq!(json["price"], serde_json::json!(7.19));
        assert_eq!(json["unit_price"], serde_json::json!(2.40));
        assert!(json["was_price"].is_null());
        // A field that is only ever noise when false stays out of the output.
        assert!(json.get("sponsored").is_none());
    }
}
