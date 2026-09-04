//! Shops, and what they hold.
//!
//! Two halves that could not be less alike. The store finder is clean JSON --
//! full records with addresses, hours and coordinates. Per-product stock is
//! HTML inside a JSON field, so it goes through [`crate::extract`].

use crate::error::{Error, Result};
use crate::wire;

/// The regions the store finder is queried by. There is no "all stores" call,
/// so listing every shop means asking for each of these.
pub const REGIONS: [(&str, &str); 16] = [
    ("NZ-NTL", "Northland"),
    ("NZ-AUK", "Auckland"),
    ("NZ-WKO", "Waikato"),
    ("NZ-BOP", "Bay of Plenty"),
    ("NZ-GIS", "Gisborne"),
    ("NZ-HKB", "Hawke's Bay"),
    ("NZ-TKI", "Taranaki"),
    ("NZ-MWT", "Manawatu-Whanganui"),
    ("NZ-WGN", "Wellington"),
    ("NZ-TAS", "Tasman"),
    ("NZ-NSN", "Nelson"),
    ("NZ-MBH", "Marlborough"),
    ("NZ-WTC", "West Coast"),
    ("NZ-CAN", "Canterbury"),
    ("NZ-OTA", "Otago"),
    ("NZ-STL", "Southland"),
];

/// Whether a region code is one the finder knows, so a typo is caught before a
/// request that would answer with an empty list.
pub fn is_region(code: &str) -> bool {
    REGIONS.iter().any(|(c, _)| c.eq_ignore_ascii_case(code))
}

/// Resolve a region by code or by name, so `--region canterbury` works as well
/// as `--region NZ-CAN`.
pub fn region(needle: &str) -> Option<&'static str> {
    let needle = needle.trim();
    REGIONS
        .iter()
        .find(|(code, name)| code.eq_ignore_ascii_case(needle) || name.eq_ignore_ascii_case(needle))
        .map(|(code, _)| *code)
}

pub fn parse(body: &str) -> Result<Vec<crate::Store>> {
    let parsed: wire::StoresResponse =
        serde_json::from_str(body).map_err(|e| Error::decode("parsing the store list", e))?;
    Ok(parsed
        .stores
        .stores
        .into_iter()
        .map(crate::Store::from)
        .collect())
}

/// Per-store stock, out of whichever field the endpoint used.
///
/// `Product-GetPDPStoreStockLevels` calls it `modalTemplate` and
/// `Stores-PDPStoreAvailability` calls it `stores`; both hold the same markup.
pub fn parse_stock(body: &str) -> Result<Vec<crate::StoreStock>> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| Error::decode("parsing store stock", e))?;
    let html = ["modalTemplate", "stores", "renderedRegionStores"]
        .iter()
        .find_map(|k| value.get(*k).and_then(|v| v.as_str()))
        .ok_or_else(|| Error::not_in_page("the store availability markup"))?;
    Ok(crate::extract::store_stock(html))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_is_found_by_code_or_by_name() {
        assert_eq!(region("nz-can"), Some("NZ-CAN"));
        assert_eq!(region("Canterbury"), Some("NZ-CAN"));
        assert_eq!(region("Chatham Islands"), None);
        assert!(is_region("NZ-AUK"));
    }

    #[test]
    fn both_endpoints_names_for_the_same_markup_are_read() {
        // One field name per controller for identical content; keying on one
        // would silently return nothing for the other.
        let markup = r##"<div class="store panel">
            <div data-target="#c-full-store-details-116">
            <h6 class="title">Example Town</h6>
            <span class="store-availability store-availability__IN_STOCK">In stock</span>
            </div></div>"##;
        for key in ["modalTemplate", "stores"] {
            // Through `serde_json` rather than a hand-escaped literal: the
            // markup arrives as a JSON string value, and escaping it by hand in
            // the test would be testing the escaping rather than the parse.
            let body = serde_json::json!({ "action": "X", key: markup }).to_string();
            let stock = parse_stock(&body).unwrap();
            assert_eq!(stock.len(), 1, "{key}");
            assert_eq!(stock[0].store_name, "Example Town");
            assert_eq!(stock[0].in_stock, Some(true));
        }
    }

    #[test]
    fn a_response_carrying_no_markup_is_an_error_not_an_empty_list() {
        // An empty list would read as "no store has it", which is a different
        // and much worse claim than "the response did not parse".
        let err = parse_stock(r#"{"action":"X"}"#).unwrap_err();
        assert!(matches!(err, Error::NotInPage { .. }), "{err}");
    }

    #[test]
    fn a_store_list_parses_to_records() {
        let body = r#"{"stores":{"stores":[
            {"ID":"119","name":"Example Town","stateCode":"NZ-AUK","fullAddress":"1 Example Rd"}]}}"#;
        let stores = parse(body).unwrap();
        assert_eq!(stores[0].id, "119");
        assert_eq!(stores[0].region.as_deref(), Some("NZ-AUK"));
    }
}
