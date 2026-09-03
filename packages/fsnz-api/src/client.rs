//! The edge API the storefronts' own frontends call.

use net_kit::wreq;
use serde::de::DeserializeOwned;

use crate::banner::{Banner, Endpoints};
use crate::domain::{Category, Product, Store};
use crate::error::{Error, Result};
use crate::wire::{WireCategory, WireProduct, WireSearchPage, WireStore};

/// The site's own default ordering. Passed through verbatim so an unfamiliar
/// value reaches the API rather than being rejected here.
pub const DEFAULT_SORT: &str = "NI_POPULARITY_ASC";

/// The API's own page ceiling for the search endpoint.
const MAX_HITS_PER_PAGE: u32 = 50;

pub struct Client {
    http: wreq::Client,
    banner: Banner,
    endpoints: Endpoints,
    token: String,
}

pub struct SearchResult {
    pub products: Vec<Product>,
    pub total_available: u32,
}

impl Client {
    pub fn new(
        http: wreq::Client,
        banner: Banner,
        endpoints: Endpoints,
        token: impl Into<String>,
    ) -> Client {
        Client {
            http,
            banner,
            endpoints,
            token: token.into(),
        }
    }

    pub fn banner(&self) -> Banner {
        self.banner
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    fn headers(&self) -> wreq::header::HeaderMap {
        use wreq::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, ORIGIN, REFERER};
        // No User-Agent: the emulation sets one that matches the handshake, and
        // overriding it here would make the two disagree.
        let mut h = HeaderMap::new();
        h.insert(ACCEPT, HeaderValue::from_static("application/json"));
        if let Ok(v) = HeaderValue::from_str(&self.endpoints.origin) {
            h.insert(ORIGIN, v);
        }
        if let Ok(v) = HeaderValue::from_str(&format!("{}/", self.endpoints.origin)) {
            h.insert(REFERER, v);
        }
        if let Ok(mut v) = HeaderValue::from_str(&format!("Bearer {}", self.token)) {
            v.set_sensitive(true);
            h.insert(AUTHORIZATION, v);
        }
        h
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{path}", self.endpoints.api);
        let sent = self.http.get(&url).headers(self.headers()).send().await;
        Ok(net_kit::http::json("GET", &url, sent).await?)
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        let url = format!("{}{path}", self.endpoints.api);
        let sent = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(body)
            .send()
            .await;
        Ok(net_kit::http::json("POST", &url, sent).await?)
    }

    /// Every store the banner runs. Prices and stock are per store, which is
    /// why one has to be chosen before searching.
    ///
    /// Guest-scoped: an account token gets a flat 400 here, which is why guest
    /// and account tokens are cached in separate files.
    pub async fn stores(&self) -> Result<Vec<Store>> {
        let raw: serde_json::Value = self.get("/v1/edge/store").await?;
        // Seen both as a bare array and wrapped in `{ "stores": [...] }`.
        let items = match raw {
            serde_json::Value::Array(items) => items,
            serde_json::Value::Object(ref map) => match map.get("stores") {
                Some(serde_json::Value::Array(items)) => items.clone(),
                _ => return Err(Error::Shape("store list had no 'stores' array".into())),
            },
            _ => return Err(Error::Shape("unexpected store list response".into())),
        };

        Ok(items
            .into_iter()
            .filter_map(|item| serde_json::from_value::<WireStore>(item).ok())
            .filter_map(|w| {
                Some(Store {
                    id: w.id?,
                    name: w.name.unwrap_or_else(|| "(unnamed)".into()),
                    banner: self.banner,
                    region: w.region,
                    address: w.address.and_then(|a| a.as_str().map(str::to_string)),
                })
            })
            .collect())
    }

    /// The department tree for one store.
    ///
    /// Store-scoped, unlike the equivalent at the other chain: the same tree
    /// differs between stores because it is built from what that store ranges.
    pub async fn categories(&self, store_id: &str) -> Result<Vec<Category>> {
        let raw: Vec<WireCategory> = self
            .get(&format!("/v1/edge/store/{store_id}/categories"))
            .await?;
        Ok(raw.into_iter().filter_map(into_category).collect())
    }

    async fn search_page(
        &self,
        store_id: &str,
        query: &str,
        filters: &str,
        page: u32,
        hits_per_page: u32,
        sort: &str,
    ) -> Result<WireSearchPage> {
        let body = serde_json::json!({
            "algoliaQuery": {
                "attributesToHighlight": [],
                "attributesToRetrieve": ["productID", "Type"],
                "facets": ["brand", "category1NI", "onPromotion"],
                "filters": filters,
                "hitsPerPage": hits_per_page,
                "maxValuesPerFacet": 100,
                "page": page,
                "query": query,
            },
            "algoliaFacetQueries": [],
            "storeId": store_id,
            "hitsPerPage": hits_per_page,
            "page": page,
            "sortOrder": sort,
            "tobaccoQuery": true,
        });
        self.post("/v1/edge/search/paginated/products", &body).await
    }

    /// Page until `max` products are in hand or the results run out.
    ///
    /// Pages are numbered from zero here, unlike the order endpoint's.
    pub async fn collect(
        &self,
        store_id: &str,
        query: &str,
        filters: &str,
        max: u32,
        sort: &str,
    ) -> Result<SearchResult> {
        let per_page = max.clamp(1, MAX_HITS_PER_PAGE);
        let mut products: Vec<Product> = Vec::new();
        let mut page = 0u32;
        let mut total_pages = 1u32;
        let mut total_available = 0u32;

        while (products.len() as u32) < max && page < total_pages {
            let res = self
                .search_page(store_id, query, filters, page, per_page, sort)
                .await?;
            let items = res.products.unwrap_or_default();
            total_available = res
                .total_hits
                .unwrap_or(products.len() as u32 + items.len() as u32);
            total_pages = res.total_pages.unwrap_or(page + 1);
            if items.is_empty() {
                break;
            }
            let room = max as usize - products.len();
            products.extend(items.into_iter().take(room).map(|p| self.map_product(p)));
            page += 1;
        }

        Ok(SearchResult {
            products,
            total_available,
        })
    }

    fn map_product(&self, p: WireProduct) -> Product {
        let single = p.single_price.as_ref();
        let comparative = single.and_then(|s| s.comparative_price.as_ref());
        // `productId` looks like "5010819-EA-000"; the leading number is the
        // key the image CDN is addressed by.
        let numeric = p.product_id.as_deref().and_then(|id| id.split('-').next());

        let multi_buy = p
            .promotions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|pr| pr.threshold.unwrap_or(1) > 1)
            .and_then(|pr| match (pr.threshold, pr.reward_value) {
                (Some(t), Some(v)) => Some(format!("{t} for {}", dollars(v))),
                _ => None,
            });

        // `price` is already the promo price when something is on special; the
        // search response carries no "was" price, so there is nothing to strike
        // through.
        let is_special = single.and_then(|s| s.promo_id.as_ref()).is_some()
            || p.promotions.as_deref().is_some_and(|v| !v.is_empty());

        let sku = p.product_id.clone().unwrap_or_default();
        Product {
            url: format!("{}/shop/product/{sku}", self.endpoints.origin),
            image: numeric
                .map(|n| format!("https://a.fsimg.co.nz/product/retail/fan/image/200x200/{n}.png")),
            sku,
            banner: self.banner,
            name: p.name.unwrap_or_default(),
            brand: p.brand.filter(|b| !b.trim().is_empty()),
            size: p.display_name.filter(|d| !d.trim().is_empty()),
            price_cents: single.and_then(|s| s.price),
            unit_price_cents: comparative.and_then(|c| c.price_per_unit),
            unit_measure: comparative.and_then(|c| {
                c.measure_description
                    .clone()
                    .or_else(|| c.unit_quantity_uom.clone())
            }),
            multi_buy,
            is_special,
            in_stock: p
                .availability
                .as_ref()
                .map(|a| a.iter().any(|s| s.eq_ignore_ascii_case("ONLINE"))),
            department: p
                .category_trees
                .as_deref()
                .unwrap_or_default()
                .first()
                .and_then(|t| t.level0.clone()),
        }
    }
}

fn into_category(w: WireCategory) -> Option<Category> {
    Some(Category {
        name: w.name.filter(|n| !n.trim().is_empty())?,
        children: w.children.into_iter().filter_map(into_category).collect(),
    })
}

/// Only used for the pre-rendered multi-buy label.
fn dollars(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let cents = cents.unsigned_abs();
    format!("{sign}${}.{:02}", cents / 100, cents % 100)
}

/// Algolia-style filter expression. Every search is scoped to a store; the
/// extras narrow it further.
pub fn filters(store_id: &str, specials_only: bool, department: Option<&str>) -> String {
    let mut f = format!("stores:{store_id}");
    if let Some(dept) = department.map(str::trim).filter(|d| !d.is_empty()) {
        // Quoted because department names contain spaces and ampersands.
        f.push_str(&format!(" AND category0NI:\"{}\"", dept.replace('"', "")));
    }
    if specials_only {
        f.push_str(&format!(" AND onPromotion:{store_id}"));
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_are_always_scoped_to_a_store() {
        assert_eq!(filters("s1", false, None), "stores:s1");
        assert_eq!(filters("s1", true, None), "stores:s1 AND onPromotion:s1");
        assert_eq!(
            filters("s1", false, Some("Fruit & Vegetables")),
            "stores:s1 AND category0NI:\"Fruit & Vegetables\""
        );
    }

    #[test]
    fn a_quote_in_a_department_name_cannot_break_the_expression() {
        assert_eq!(
            filters("s1", false, Some("Say \"cheese\"")),
            "stores:s1 AND category0NI:\"Say cheese\""
        );
    }

    #[test]
    fn money_puts_the_sign_outside_the_symbol() {
        assert_eq!(dollars(500), "$5.00");
        assert_eq!(dollars(-250), "-$2.50");
    }

    #[test]
    fn a_category_without_a_name_is_dropped_not_rendered_blank() {
        let node = WireCategory {
            name: Some("  ".into()),
            children: Vec::new(),
        };
        assert!(into_category(node).is_none());
    }

    #[test]
    fn category_children_survive_the_conversion() {
        let node = WireCategory {
            name: Some("Fridge, Deli & Eggs".into()),
            children: vec![WireCategory {
                name: Some("Milk".into()),
                children: Vec::new(),
            }],
        };
        let c = into_category(node).unwrap();
        assert_eq!(c.name, "Fridge, Deli & Eggs");
        assert_eq!(c.children[0].name, "Milk");
    }
}
