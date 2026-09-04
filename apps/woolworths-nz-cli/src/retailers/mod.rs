//! The adapter, and the thing that hands it out.
//!
//! `packages/` holds `wwnz-api`, which knows nothing about the domain, and
//! `gsnz-core`, which knows nothing about HTTP. Everything that reconciles them
//! lives here: the [`Retailer`] implementation, its conversions, and a [`Lazy`]
//! that builds it on demand.

pub mod woolworths;

use std::sync::{Arc, Mutex};

use gsnz_core::{Error, Result, Retailer, Search, Store};

use crate::app::RETAILER;

pub type Handle = Arc<dyn Retailer>;

/// The adapter, built once and only when something asks for it.
///
/// `wwnz completions bash` must not pay for a client it will not use -- which
/// matters more than it sounds, because building one loads a cookie jar out of
/// the keyring and may prompt for the login keychain.
pub struct Lazy {
    build: Box<dyn Fn() -> Result<Handle> + Send + Sync>,
    made: Mutex<Option<Handle>>,
}

impl Lazy {
    pub fn new(build: impl Fn() -> Result<Handle> + Send + Sync + 'static) -> Lazy {
        Lazy {
            build: Box::new(build),
            made: Mutex::new(None),
        }
    }

    pub fn get(&self) -> Result<Handle> {
        if let Some(handle) = self.made.lock().expect("adapter lock").as_ref() {
            return Ok(handle.clone());
        }
        let handle = (self.build)()?;
        *self.made.lock().expect("adapter lock") = Some(handle.clone());
        Ok(handle)
    }
}

/// Filter a store list the way a person expects `wwnz stores wellington` to.
///
/// The API filters server-side and this filters again, so a query that the
/// server widened -- or one it ignored -- still narrows to what was asked for.
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
pub fn resolve_store(stores: Vec<Store>, needle: &str) -> Result<Store> {
    let needle = needle.trim();
    if let Some(store) = stores.iter().find(|s| s.id == needle) {
        return Ok(store.clone());
    }
    let matches: Vec<&Store> = stores.iter().filter(|s| s.matches(needle)).collect();
    match matches.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(Error::Other(format!(
            "no {RETAILER} store matching {needle:?}: run `wwnz stores` to list them"
        ))),
        many => Err(Error::Other(format!(
            "{} {RETAILER} stores match {needle:?}: {}",
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
/// `--size` is filtered here rather than at the server -- the API has no size
/// facet worth using -- so a request that will throw some away asks for more,
/// and `--size 2l --limit 5` still has five to show.
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
    use gsnz_core::{RetailerId, SearchBy};

    fn store(id: &str, name: &str, city: &str) -> Store {
        Store {
            retailer: RetailerId::Woolworths,
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
        let stores = vec![store("9048", "Regent", "Whangarei")];
        assert_eq!(resolve_store(stores, "9048").unwrap().id, "9048");
    }

    #[test]
    fn an_ambiguous_name_is_refused_rather_than_guessed_at() {
        // Picking the first would bind the cart to the wrong shop, and the only
        // symptom would be a price that is slightly off.
        let stores = vec![
            store("s1", "Thorndon", "Wellington"),
            store("s2", "Newtown", "Wellington"),
        ];
        let err = resolve_store(stores, "wellington").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("2 Woolworths stores"), "{text}");
        assert!(text.contains("Thorndon (s1)"), "{text}");
    }

    #[test]
    fn nothing_matching_says_how_to_find_out() {
        let err = resolve_store(Vec::new(), "nowhere").unwrap_err();
        assert!(err.to_string().contains("wwnz stores"), "{err}");
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
