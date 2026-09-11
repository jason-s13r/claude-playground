//! Search and browse, which is Klevu rather than Magento.
//!
//! The storefront hands its product grid to Klevu and keeps Magento for the
//! product page, so this crate does the same: results here are the ones the
//! website would show, in the order it would show them. Magento *can* answer
//! `products(search:)` and it is a perfectly good keyword search, but it ranks
//! differently and knows nothing about the facets the site offers, so a listing
//! built on it would quietly disagree with the site it is meant to mirror.
//!
//! Klevu needs no credentials of any kind -- the API key is public and the
//! storefront publishes it. What it does need is the **numeric** category id,
//! not Magento's base64 uid: the `ancestors` condition filters on `"11850"`
//! where the same category is `MTE4NTA=` everywhere else. [`crate::wire::decode_uid`]
//! is the bridge, and [`Query::category`] takes either spelling so a caller
//! never has to know.

use serde::Serialize;

use crate::domain::{Facet, FacetOption, Listing};
use crate::wire::{decode_uid, KlevuEnvelope};
use crate::Banner;

/// How many products a page holds by default. The site asks for 36 browsing
/// and 35 searching; there is no reason for the difference and one number is
/// easier to reason about.
pub const PAGE_SIZE: u64 = 36;

/// The orderings Klevu accepts. Passed through as written -- these are index
/// names rather than anything this crate interprets.
pub const SORTS: &[&str] = &[
    "RELEVANCE",
    "PRICE_ASC",
    "PRICE_DESC",
    "NAME_ASC",
    "NAME_DESC",
    "NEW_ARRIVAL",
];

pub const DEFAULT_SORT: &str = "RELEVANCE";

/// The fields worth asking the index for.
///
/// A subset of the site's twenty-nine. Klevu returns exactly what is listed and
/// nothing else, so this is the one place that decides what a [`crate::domain::Product`]
/// can carry.
const FIELDS: &[&str] = &[
    "sku",
    "name",
    "brand",
    "price",
    "salePrice",
    "basePrice",
    "inStock",
    "url",
    "image",
];

/// What to ask the index.
#[derive(Clone, Debug, Default)]
pub struct Query {
    term: Option<String>,
    category: Option<String>,
    sort: Option<String>,
    limit: Option<u64>,
    offset: u64,
    in_stock_only: bool,
}

impl Query {
    /// A keyword search.
    pub fn search(term: impl Into<String>) -> Query {
        Query {
            term: Some(term.into()),
            ..Default::default()
        }
    }

    /// Everything under a category.
    ///
    /// Takes a Magento uid or the bare numeric id; both reach the same place.
    /// An empty term with an `ancestors` condition is how the site browses, and
    /// it is not the same request as searching for the category's name.
    pub fn category(id: impl Into<String>) -> Query {
        let id = id.into();
        Query {
            term: Some(String::new()),
            category: Some(decode_uid(&id).unwrap_or(id)),
            ..Default::default()
        }
    }

    pub fn with_sort(mut self, sort: Option<String>) -> Query {
        self.sort = sort;
        self
    }

    pub fn with_limit(mut self, limit: Option<u64>) -> Query {
        self.limit = limit;
        self
    }

    pub fn with_offset(mut self, offset: u64) -> Query {
        self.offset = offset;
        self
    }

    /// Drop products the index knows are out of stock.
    ///
    /// The site sets this on both its search and its browse grids, so leaving
    /// it off is what makes a listing here disagree with the website.
    pub fn in_stock_only(mut self, only: bool) -> Query {
        self.in_stock_only = only;
        self
    }

    pub fn limit(&self) -> u64 {
        self.limit.unwrap_or(PAGE_SIZE)
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The request body, which is one "record query" in a list of them.
    ///
    /// Klevu's shape is built for a page asking several unrelated questions at
    /// once -- the grid, the autosuggestions and the banners. This asks one.
    pub fn body(&self, api_key: &str) -> serde_json::Value {
        let mut prefs = vec!["searchCompoundsAsAndQuery"];
        if self.in_stock_only {
            prefs.push("hideOutOfStockProducts");
        }
        let conditions: Vec<_> = self
            .category
            .iter()
            .map(|id| {
                serde_json::json!({
                    "key": "ancestors",
                    "values": [id],
                    "singleSelect": false,
                    "valueOperator": "INCLUDE",
                })
            })
            .collect();

        serde_json::json!({
            "context": { "apiKeys": [api_key] },
            "recordQueries": [{
                "id": ID,
                "typeOfRequest": "SEARCH",
                "settings": {
                    "query": { "term": self.term.clone().unwrap_or_default() },
                    "id": ID,
                    "limit": self.limit(),
                    "offset": self.offset,
                    "typeOfRecords": ["KLEVU_PRODUCT"],
                    "searchPrefs": prefs,
                    "sort": self.sort.clone().unwrap_or_else(|| DEFAULT_SORT.to_string()),
                    "fields": FIELDS,
                    "groupCondition": {
                        "groupOperator": "ALL_OF",
                        "conditions": conditions,
                    },
                },
                "filters": {
                    "filtersToReturn": {
                        "enabled": true,
                        "options": { "order": "FREQ", "limit": 50 },
                        "rangeFilterSettings": [{ "key": "klevu_price", "minMax": true }],
                    },
                },
            }],
        })
    }
}

/// The id this crate gives its one record query, and the id it reads back.
///
/// Klevu answers a list keyed by whatever ids were asked, so the two have to
/// agree; naming it once is what makes that true by construction.
const ID: &str = "productList";

/// Shape an answer, ignoring any record query that was not the one asked.
pub fn listing(banner: Banner, body: &str) -> Result<Listing, serde_json::Error> {
    let envelope: KlevuEnvelope = serde_json::from_str(body)?;
    let result = envelope
        .query_results
        .iter()
        .find(|r| r.id.as_deref() == Some(ID))
        .or_else(|| envelope.query_results.first());

    let Some(result) = result else {
        return Ok(Listing {
            banner,
            products: Vec::new(),
            total: 0,
            offset: 0,
            facets: Vec::new(),
        });
    };

    Ok(Listing {
        banner,
        products: result
            .records
            .iter()
            .filter_map(crate::wire::KlevuRecord::product)
            .collect(),
        total: result.meta.total_results_found,
        offset: result.meta.offset,
        facets: result
            .filters
            .iter()
            .filter_map(|f| {
                let key = f.key.clone()?;
                Some(Facet {
                    label: f.label.clone().unwrap_or_else(|| key.clone()),
                    key,
                    options: f
                        .options
                        .iter()
                        .filter_map(|o| {
                            Some(FacetOption {
                                value: o.name.clone().or_else(|| o.value.clone())?,
                                count: o.count.map(|c| c as u64),
                            })
                        })
                        .collect(),
                })
            })
            .collect(),
    })
}

/// A serialisable echo of a query, for `--json` and for tests.
#[derive(Serialize)]
pub struct QueryEcho<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<&'a str>,
    pub offset: u64,
    pub limit: u64,
}

impl Query {
    pub fn echo(&self) -> QueryEcho<'_> {
        QueryEcho {
            term: self.term.as_deref().filter(|t| !t.is_empty()),
            category: self.category.as_deref(),
            offset: self.offset,
            limit: self.limit(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsing_a_category_sends_the_numeric_id_however_it_was_given() {
        // Magento calls this category MTE4NTA=; Klevu only knows 11850.
        let from_uid = Query::category("MTE4NTA=").body("klevu-test");
        let from_id = Query::category("11850").body("klevu-test");
        let condition = |b: &serde_json::Value| {
            b["recordQueries"][0]["settings"]["groupCondition"]["conditions"][0]["values"][0]
                .as_str()
                .map(str::to_string)
        };
        assert_eq!(condition(&from_uid).as_deref(), Some("11850"));
        assert_eq!(condition(&from_id).as_deref(), Some("11850"));
    }

    #[test]
    fn browsing_sends_an_empty_term_rather_than_the_category_name() {
        // Searching for "Tudo Home" and browsing the Tudo Home category are
        // different questions with different answers.
        let body = Query::category("11850").body("klevu-test");
        assert_eq!(
            body["recordQueries"][0]["settings"]["query"]["term"]
                .as_str()
                .expect("a term is always sent"),
            ""
        );
    }

    #[test]
    fn searching_sends_no_category_condition() {
        let body = Query::search("broom").body("klevu-test");
        let conditions = &body["recordQueries"][0]["settings"]["groupCondition"]["conditions"];
        assert_eq!(conditions.as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn hiding_out_of_stock_is_opt_in_and_reaches_the_wire() {
        let plain = Query::search("broom").body("k");
        let prefs = |b: &serde_json::Value| {
            b["recordQueries"][0]["settings"]["searchPrefs"]
                .as_array()
                .expect("prefs are always sent")
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        };
        assert!(!prefs(&plain).iter().any(|p| p == "hideOutOfStockProducts"));

        let hidden = Query::search("broom").in_stock_only(true).body("k");
        assert!(prefs(&hidden).iter().any(|p| p == "hideOutOfStockProducts"));
    }

    #[test]
    fn the_api_key_is_the_one_it_was_given() {
        // It comes from storeConfig and differs per fascia; a hardcoded one
        // here would answer with the wrong catalogue, or with nothing.
        let body = Query::search("x").body("klevu-173190029190817559");
        assert_eq!(
            body["context"]["apiKeys"][0].as_str(),
            Some("klevu-173190029190817559")
        );
    }

    #[test]
    fn an_answer_is_read_back_off_the_id_it_was_asked_under() {
        let body = r#"{"meta":{"responseCode":200},"queryResults":[
          {"id":"autosuggestion","meta":{"totalResultsFound":0,"offset":0},"records":[]},
          {"id":"productList","meta":{"totalResultsFound":173,"offset":36},
           "records":[{"sku":"1103832","name":"Jug","salePrice":"79.99","price":"79.99","inStock":"yes"}],
           "filters":[{"key":"brand","label":"Brand","options":[{"name":"Prestige","count":12}]}]}
        ]}"#;
        let listing = listing(Banner::Briscoes, body).expect("a Klevu answer parses");
        assert_eq!(listing.total, 173);
        assert_eq!(listing.offset, 36);
        assert_eq!(
            listing.products.len(),
            1,
            "the suggestions block is not products"
        );
        assert_eq!(listing.products[0].sku, "1103832");
        assert_eq!(listing.facets.len(), 1);
        assert_eq!(listing.facets[0].options[0].count, Some(12));
    }

    #[test]
    fn an_answer_with_no_record_queries_is_an_empty_listing_not_a_failure() {
        let listing =
            listing(Banner::RebelSport, r#"{"meta":{},"queryResults":[]}"#).expect("parses");
        assert!(listing.products.is_empty());
        assert_eq!(listing.total, 0);
    }
}
