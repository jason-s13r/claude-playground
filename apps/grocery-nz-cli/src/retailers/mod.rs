//! The adapters, and the thing that hands them out.
//!
//! `packages/` holds two API crates that know nothing about each other and
//! nothing about the domain. Everything that reconciles them lives here: two
//! [`Retailer`] implementations, their conversions, and a [`Registry`] that
//! builds one on demand.

pub mod foodstuffs;
pub mod woolworths;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gsnz_core::{Error, Result, Retailer, RetailerId, Search, Store};

pub type Handle = Arc<dyn Retailer>;

/// Adapters, built once each and only when asked for.
///
/// `gsnz -b nw search` must not pay for a Woolworths client it will not use --
/// which matters more than it sounds, because building one loads a cookie jar
/// out of the keyring and may prompt for the login keychain.
pub struct Registry {
    build: Box<dyn Fn(RetailerId) -> Result<Handle> + Send + Sync>,
    made: Mutex<HashMap<RetailerId, Handle>>,
}

impl Registry {
    pub fn new(build: impl Fn(RetailerId) -> Result<Handle> + Send + Sync + 'static) -> Registry {
        Registry {
            build: Box::new(build),
            made: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, id: RetailerId) -> Result<Handle> {
        if let Some(handle) = self.made.lock().expect("registry lock").get(&id) {
            return Ok(handle.clone());
        }
        let handle = (self.build)(id)?;
        self.made
            .lock()
            .expect("registry lock")
            .insert(id, handle.clone());
        Ok(handle)
    }
}

/// Filter a store list the way a person expects `gsnz stores wellington` to.
///
/// Both APIs answer a store query differently -- Woolworths filters server-side
/// and Foodstuffs returns everything -- so the same filter is applied here
/// either way and the result is the same for both.
pub fn narrow_stores(stores: Vec<Store>, query: Option<&str>, max: u32) -> Vec<Store> {
    let mut stores = stores;
    if let Some(needle) = query.map(str::trim).filter(|q| !q.is_empty()) {
        stores.retain(|s| s.matches(needle));
    }
    stores.truncate(max as usize);
    stores
}

/// Turn what someone typed into exactly one store.
///
/// An id wins outright. Failing that a name has to be unambiguous: picking the
/// first of four Wellington stores would bind the cart to the wrong one, and
/// the mistake would only show up as a price.
pub fn resolve_store(stores: Vec<Store>, needle: &str, id: RetailerId) -> Result<Store> {
    let needle = needle.trim();
    if let Some(store) = stores.iter().find(|s| s.id == needle) {
        return Ok(store.clone());
    }
    let matches: Vec<&Store> = stores.iter().filter(|s| s.matches(needle)).collect();
    match matches.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(Error::Other(format!(
            "no {id} store matching {needle:?}: run `gsnz -b {} stores` to list them",
            id.short()
        ))),
        many => Err(Error::Other(format!(
            "{} {id} stores match {needle:?}: {}",
            many.len(),
            many.iter()
                .take(5)
                .map(|s| format!("{} ({})", s.name, s.id))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// How many products to ask the API for.
///
/// `--size` is filtered here rather than at the server -- neither API has a
/// size facet worth using -- so a request that will throw some away asks for
/// more, and `--size 2l --limit 5` still has five to show.
pub fn fetch_limit(search: &Search) -> u32 {
    if search.size.is_some() {
        search.limit.saturating_mul(5).clamp(1, 200)
    } else {
        search.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gsnz_core::SearchBy;

    fn store(id: &str, name: &str, city: &str) -> Store {
        Store {
            retailer: RetailerId::NewWorld,
            id: id.into(),
            name: name.into(),
            address: None,
            area: None,
            city: Some(city.into()),
            distance_km: None,
        }
    }

    #[test]
    fn an_id_is_taken_at_its_word() {
        let stores = vec![store("s1", "Thorndon", "Wellington")];
        assert_eq!(
            resolve_store(stores, "s1", RetailerId::NewWorld)
                .unwrap()
                .id,
            "s1"
        );
    }

    #[test]
    fn an_ambiguous_name_is_refused_rather_than_guessed_at() {
        // Picking the first would bind the cart to the wrong shop, and the only
        // symptom would be a price that is slightly off.
        let stores = vec![
            store("s1", "Thorndon", "Wellington"),
            store("s2", "Newtown", "Wellington"),
        ];
        let err = resolve_store(stores, "wellington", RetailerId::NewWorld).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("2 New World stores"), "{text}");
        assert!(text.contains("Thorndon (s1)"), "{text}");
    }

    #[test]
    fn nothing_matching_says_how_to_find_out() {
        let err = resolve_store(Vec::new(), "nowhere", RetailerId::Woolworths).unwrap_err();
        assert!(err.to_string().contains("gsnz -b ww stores"), "{err}");
    }

    #[test]
    fn a_size_filter_over_fetches_so_the_limit_can_still_be_met() {
        let mut search = Search::new(SearchBy::Query("milk".into()));
        search.limit = 10;
        assert_eq!(fetch_limit(&search), 10);
        search.size = Some("2l".into());
        assert_eq!(fetch_limit(&search), 50);
    }
}
