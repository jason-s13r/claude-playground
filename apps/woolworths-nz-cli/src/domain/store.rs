//! A store, once the locations response has been normalised.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Store {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suburb: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// Kilometres from the coordinate a search was centred on. Absent when the
    /// search was by name rather than by position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
}

impl Store {
    /// Match a user-typed store however they gave it: an exact id, or a
    /// case-insensitive substring of the name, suburb or address -- people look
    /// for a store by town at least as often as by name.
    pub fn matches(&self, needle: &str) -> bool {
        let needle = needle.trim();
        if self.id.eq_ignore_ascii_case(needle) {
            return true;
        }
        let needle = needle.to_lowercase();
        [
            Some(&self.name),
            self.suburb.as_ref(),
            self.city.as_ref(),
            self.address.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|field| field.to_lowercase().contains(&needle))
    }

    /// Where the store is, in one line, for a table cell.
    pub fn where_it_is(&self) -> String {
        [self.suburb.as_deref(), self.city.as_deref()]
            .into_iter()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            // Woolworths spells the suburb "Regent, Whangarei", so the city is
            // usually already in it and repeating it reads badly.
            .fold(Vec::new(), |mut acc: Vec<String>, part| {
                if !acc.iter().any(|got| got.contains(part)) {
                    acc.push(part.to_string());
                }
                acc
            })
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store {
            id: "9048".into(),
            name: "Regent Woolworths".into(),
            address: Some("11 Kamo Road".into()),
            suburb: Some("Regent, Whangarei".into()),
            city: None,
            distance_km: Some(0.62),
        }
    }

    #[test]
    fn stores_match_by_id_or_name_fragment() {
        let s = store();
        assert!(s.matches("9048"));
        assert!(s.matches("regent"));
        assert!(s.matches("REGENT"));
        assert!(s.matches("whangarei"), "town searches should work");
        assert!(s.matches("kamo road"), "address searches should work");
        assert!(!s.matches("ponsonby"));
    }

    #[test]
    fn a_location_does_not_repeat_a_town_the_suburb_already_names() {
        assert_eq!(store().where_it_is(), "Regent, Whangarei");
        let s = Store {
            suburb: Some("Ponsonby".into()),
            city: Some("Auckland".into()),
            ..store()
        };
        assert_eq!(s.where_it_is(), "Ponsonby, Auckland");
        let s = Store {
            suburb: Some("Whangarei".into()),
            city: Some("Whangarei".into()),
            ..store()
        };
        assert_eq!(s.where_it_is(), "Whangarei");
    }
}
