//! A product, once a banner's search response has been normalised.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Product {
    pub sku: String,
    pub banner: &'static str,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    /// The pack size as the site displays it ("2L", "6 pack").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(rename = "price", serialize_with = "crate::domain::money::as_dollars")]
    pub price_cents: Option<i64>,
    #[serde(
        rename = "unit_price",
        serialize_with = "crate::domain::money::as_dollars"
    )]
    pub unit_price_cents: Option<i64>,
    /// What the unit price is per: "1kg", "100ml".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_measure: Option<String>,
    /// A multi-buy offer, already rendered ("2 for $5.00").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi_buy: Option<String>,
    pub is_special: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub url: String,
}

/// Brand and name with the brand stripped off the front of the name, which the
/// catalogue repeats constantly ("Anchor Anchor Blue Milk 2L"). Order lines
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

    /// Key for lining the same product up across banners. Foodstuffs shares one
    /// catalogue between New World and PAK'nSAVE, so SKUs line up directly.
    pub fn match_key(&self) -> String {
        self.sku.to_lowercase()
    }

    pub fn matches_size(&self, needle: &str) -> bool {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        let haystack =
            format!("{} {}", self.size.as_deref().unwrap_or(""), self.name).to_lowercase();
        haystack.contains(&needle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(brand: Option<&str>, name: &str, size: Option<&str>) -> Product {
        Product {
            sku: "5010819-EA-000".into(),
            banner: "newworld",
            name: name.into(),
            brand: brand.map(str::to_string),
            size: size.map(str::to_string),
            price_cents: Some(429),
            unit_price_cents: None,
            unit_measure: None,
            multi_buy: None,
            is_special: false,
            in_stock: Some(true),
            department: None,
            image: None,
            url: String::new(),
        }
    }

    #[test]
    fn title_does_not_repeat_the_brand() {
        assert_eq!(
            product(Some("Anchor"), "Anchor Blue Milk 2L", None).title(),
            "Anchor Blue Milk 2L"
        );
        assert_eq!(
            product(Some("Anchor"), "Blue Milk 2L", None).title(),
            "Anchor Blue Milk 2L"
        );
        assert_eq!(
            product(None, "Loose Bananas", None).title(),
            "Loose Bananas"
        );
    }

    #[test]
    fn size_filter_looks_at_the_name_too() {
        let p = product(Some("Anchor"), "Anchor Blue Milk 2L", Some("2L"));
        assert!(p.matches_size("2l"));
        assert!(p.matches_size(""));
        assert!(!p.matches_size("600ml"));
    }

    #[test]
    fn json_prices_come_out_in_dollars() {
        let json = serde_json::to_value(product(None, "x", None)).unwrap();
        assert_eq!(json["price"], serde_json::json!(4.29));
        assert!(json["unit_price"].is_null());
    }
}
