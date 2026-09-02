//! A store, once a banner's response has been normalised.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Store {
    pub id: String,
    pub name: String,
    pub banner: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

impl Store {
    /// Match a user-typed store however they gave it: an exact id, or a
    /// case-insensitive substring of the name, region or address -- people
    /// look for a store by town at least as often as by name.
    pub fn matches(&self, needle: &str) -> bool {
        let needle = needle.trim();
        if self.id.eq_ignore_ascii_case(needle) {
            return true;
        }
        let needle = needle.to_lowercase();
        [
            Some(&self.name),
            self.region.as_ref(),
            self.address.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|field| field.to_lowercase().contains(&needle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_match_by_id_or_name_fragment() {
        let s = Store {
            id: "abc-123".into(),
            name: "New World Thorndon".into(),
            banner: "newworld",
            region: None,
            address: None,
        };
        assert!(s.matches("abc-123"));
        assert!(s.matches("ABC-123"));
        assert!(s.matches("thorndon"));
        assert!(!s.matches("karori"));

        let regional = Store {
            region: Some("Wellington".into()),
            ..s.clone()
        };
        assert!(regional.matches("wellington"), "town searches should work");
    }
}
