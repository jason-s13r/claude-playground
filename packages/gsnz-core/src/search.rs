//! What to ask for. One request shape covers `search`, `specials` and `browse`.

use serde::{Deserialize, Serialize};

use crate::product::Product;

/// The axis a listing runs along.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchBy {
    /// `gsnz search "milk"`
    Query(String),
    /// `gsnz browse "Fridge, Deli & Eggs"`
    Department(String),
    /// `gsnz specials` -- everything, filtered to what is on promotion.
    Everything,
}

/// Sort order, as a choice rather than a raw vendor string.
///
/// Foodstuffs wants `NI_POPULARITY_ASC` and Woolworths wants `TraderRelevance`;
/// neither belongs in a `--sort` flag a person types. `Raw` stays as the escape
/// hatch for an order we have not named yet.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sort {
    #[default]
    Relevance,
    Popularity,
    PriceAsc,
    PriceDesc,
    NameAsc,
    Raw(String),
}

impl Sort {
    pub const NAMED: [&'static str; 5] = [
        "relevance",
        "popularity",
        "price-asc",
        "price-desc",
        "name-asc",
    ];
}

impl std::str::FromStr for Sort {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().replace('_', "-").as_str() {
            "relevance" => Sort::Relevance,
            "popularity" => Sort::Popularity,
            "price-asc" | "price" | "cheapest" => Sort::PriceAsc,
            "price-desc" | "dearest" => Sort::PriceDesc,
            "name-asc" | "name" => Sort::NameAsc,
            _ => Sort::Raw(s.to_string()),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Search {
    pub by: SearchBy,
    pub specials_only: bool,
    pub limit: u32,
    /// Client-side substring filter -- neither API has a size facet worth using.
    pub size: Option<String>,
    pub sort: Sort,
}

impl Search {
    pub fn new(by: SearchBy) -> Search {
        Search {
            by,
            specials_only: false,
            limit: 20,
            size: None,
            sort: Sort::default(),
        }
    }

    /// The words being searched for, when there are any.
    pub fn term(&self) -> Option<&str> {
        match &self.by {
            SearchBy::Query(q) | SearchBy::Department(q) => Some(q),
            SearchBy::Everything => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SearchResult {
    pub products: Vec<Product>,
    /// How many the retailer says exist, which is usually more than were asked
    /// for. `None` when it does not say.
    pub total: Option<u32>,
}

impl SearchResult {
    /// Apply the client-side `--size` filter and the limit.
    pub fn narrow(mut self, search: &Search) -> SearchResult {
        if let Some(size) = search.size.as_deref() {
            self.products.retain(|p| p.matches_size(size));
        }
        self.products.truncate(search.limit as usize);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product::{Product, SaleUnit};
    use crate::retailer::RetailerId;

    fn product(name: &str, size: &str) -> Product {
        Product {
            retailer: RetailerId::NewWorld,
            sku: name.into(),
            key: name.into(),
            name: name.into(),
            brand: None,
            size: Some(size.into()),
            price_cents: Some(100),
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
    fn parses_the_names_people_type_and_keeps_the_rest_raw() {
        assert_eq!("relevance".parse::<Sort>().unwrap(), Sort::Relevance);
        assert_eq!("price-asc".parse::<Sort>().unwrap(), Sort::PriceAsc);
        assert_eq!("PRICE_ASC".parse::<Sort>().unwrap(), Sort::PriceAsc);
        assert_eq!("cheapest".parse::<Sort>().unwrap(), Sort::PriceAsc);
        assert_eq!(
            "NI_POPULARITY_ASC".parse::<Sort>().unwrap(),
            Sort::Raw("NI_POPULARITY_ASC".into()),
            "an unrecognised order passes through rather than failing"
        );
    }

    #[test]
    fn narrow_applies_the_size_filter_before_the_limit() {
        let mut search = Search::new(SearchBy::Query("milk".into()));
        search.size = Some("2L".into());
        search.limit = 10;
        let result = SearchResult {
            products: vec![product("A", "2L"), product("B", "1L"), product("C", "2L")],
            total: Some(3),
        };
        let names: Vec<String> = result
            .narrow(&search)
            .products
            .iter()
            .map(|p| p.name.clone())
            .collect();
        assert_eq!(names, ["A", "C"]);
    }

    #[test]
    fn narrow_truncates_to_the_limit() {
        let mut search = Search::new(SearchBy::Everything);
        search.limit = 2;
        let result = SearchResult {
            products: vec![product("A", "1L"), product("B", "1L"), product("C", "1L")],
            total: Some(3),
        };
        assert_eq!(result.narrow(&search).products.len(), 2);
    }
}
