//! Where the prices come from. Nothing in this program is priced until a store
//! is chosen -- both retailers price per store.

use serde::{Deserialize, Serialize};

use crate::retailer::RetailerId;

/// Just enough to name a store, for embedding in a cart or an order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreRef {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Store {
    pub retailer: RetailerId,
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    /// Suburb, region, whatever the retailer calls the smaller area.
    pub area: Option<String>,
    pub city: Option<String>,
    pub distance_km: Option<f64>,
}

impl Store {
    /// Does this store answer to what the user typed?
    ///
    /// An exact id always wins, so a store whose id happens to appear inside
    /// another store's address cannot shadow it.
    pub fn matches(&self, needle: &str) -> bool {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return false;
        }
        if self.id.to_lowercase() == needle {
            return true;
        }
        [
            Some(self.name.as_str()),
            self.area.as_deref(),
            self.city.as_deref(),
            self.address.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|field| field.to_lowercase().contains(&needle))
    }

    /// "Regent, Whangarei" -- without saying Whangarei twice when the area and
    /// the city are the same place.
    pub fn where_it_is(&self) -> Option<String> {
        match (self.area.as_deref(), self.city.as_deref()) {
            (Some(a), Some(c)) if a.eq_ignore_ascii_case(c) => Some(a.to_string()),
            (Some(a), Some(c)) => Some(format!("{a}, {c}")),
            (Some(one), None) | (None, Some(one)) => Some(one.to_string()),
            (None, None) => self.address.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(id: &str, name: &str, area: Option<&str>, city: Option<&str>) -> Store {
        Store {
            retailer: RetailerId::Woolworths,
            id: id.into(),
            name: name.into(),
            address: Some("1 Example Street".into()),
            area: area.map(Into::into),
            city: city.map(Into::into),
            distance_km: None,
        }
    }

    #[test]
    fn an_exact_id_matches() {
        let s = store("4123", "Regent", Some("Regent"), Some("Whangarei"));
        assert!(s.matches("4123"));
        assert!(!s.matches("999"));
    }

    #[test]
    fn a_name_or_place_fragment_matches_case_insensitively() {
        let s = store("4123", "Regent", Some("Regent"), Some("Whangarei"));
        assert!(s.matches("regent"));
        assert!(s.matches("WHANGAREI"));
        assert!(s.matches("example street"));
        assert!(!s.matches("auckland"));
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        let s = store("4123", "Regent", None, None);
        assert!(!s.matches(""));
        assert!(!s.matches("   "));
    }

    #[test]
    fn does_not_say_the_same_place_twice() {
        assert_eq!(
            store("1", "X", Some("Whangarei"), Some("Whangarei"))
                .where_it_is()
                .unwrap(),
            "Whangarei"
        );
        assert_eq!(
            store("1", "X", Some("Regent"), Some("Whangarei"))
                .where_it_is()
                .unwrap(),
            "Regent, Whangarei"
        );
    }
}
