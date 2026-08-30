//! The Foodstuffs edge API.
//!
//! This is the undocumented API the New World and PAK'nSAVE websites call from
//! the browser, so everything here is reverse-engineered and everything is
//! optional on the way in -- a field Foodstuffs renames should degrade to a
//! missing column, not a failed command.

mod wire;

use anyhow::{bail, Context, Result};

use crate::api::wire::{WireProduct, WireSearchPage, WireStore};
use crate::banner::{Banner, Endpoints};
use crate::domain::cart::{self, Cart, Change, WireCart};
use crate::domain::order::{
    Order, OrderLine, OrderPage, Source, WireOrderDetail, WireOrderPage, WirePreviousPurchases,
};
use crate::domain::Product;
use crate::domain::Store;
use crate::token::USER_AGENT;

/// The site's own default ordering. Passed through verbatim so an unfamiliar
/// value from `--sort` reaches the API rather than being rejected here.
pub const DEFAULT_SORT: &str = "NI_POPULARITY_ASC";

/// The API's own page ceiling for this endpoint.
const MAX_HITS_PER_PAGE: u32 = 50;

/// The site asks for 20 orders a page. Nothing documents a ceiling, so this
/// stays at what is known to work and pages for the rest.
const MAX_ORDERS_PER_PAGE: u32 = 20;

pub struct Client {
    http: reqwest::Client,
    banner: Banner,
    endpoints: Endpoints,
    token: String,
}

impl Client {
    pub fn new(
        http: reqwest::Client,
        banner: Banner,
        endpoints: Endpoints,
        token: String,
    ) -> Client {
        Client {
            http,
            banner,
            endpoints,
            token,
        }
    }

    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.endpoints.api);
        let res = self
            .http
            .get(&url)
            .headers(self.common_headers())
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        self.read_json(res, &url).await
    }

    async fn post(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.endpoints.api);
        let res = self
            .http
            .post(&url)
            .headers(self.common_headers())
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;
        self.read_json(res, &url).await
    }

    fn common_headers(&self) -> reqwest::header::HeaderMap {
        use reqwest::header::{
            HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, ORIGIN, REFERER, USER_AGENT as UA,
        };
        let mut h = HeaderMap::new();
        h.insert(UA, HeaderValue::from_static(USER_AGENT));
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

    async fn read_json(&self, res: reqwest::Response, url: &str) -> Result<serde_json::Value> {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        if !status.is_success() {
            let detail = body.trim();
            let detail = if detail.is_empty() {
                String::new()
            } else {
                format!(": {}", truncate(detail, 300))
            };
            let hint = if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                "\nThe token may be expired or rejected; try `fsnz auth refresh`."
            } else {
                ""
            };
            bail!(
                "{} API {status} for {url}{detail}{hint}",
                self.banner.name()
            );
        }
        serde_json::from_str(&body).with_context(|| {
            format!(
                "{url} returned a body that is not JSON: {}",
                truncate(&body, 200)
            )
        })
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{path}", self.endpoints.api);
        let res = self
            .http
            .delete(&url)
            .headers(self.common_headers())
            .send()
            .await
            .with_context(|| format!("DELETE {url}"))?;
        self.read_json(res, &url).await.map(|_| ())
    }

    // ---- cart ----
    // The cart belongs to an account rather than a store, so all of this needs
    // a logged-in token; a guest one gets a 401.

    pub async fn cart(&self) -> Result<Cart> {
        let raw = self
            .get("/v1/edge/cart")
            .await
            .map_err(needs_login("the cart"))?;
        Ok(serde_json::from_value::<WireCart>(raw)
            .context("parsing the cart")?
            .into_cart())
    }

    /// Apply changes and return the cart as it stands afterwards. A quantity of
    /// zero removes a line, which is how the site does it too.
    pub async fn cart_apply(&self, changes: &[Change]) -> Result<Cart> {
        let body = cart::changes_body(changes)?;
        let raw = self
            .post("/v1/edge/cart", &body)
            .await
            .map_err(needs_login("the cart"))?;
        Ok(serde_json::from_value::<WireCart>(raw)
            .context("parsing the updated cart")?
            .into_cart())
    }

    pub async fn cart_clear(&self) -> Result<()> {
        self.delete("/v1/edge/cart")
            .await
            .map_err(needs_login("the cart"))
    }

    // ---- orders ----
    // History belongs to an account too, so like the cart it needs a logged-in
    // token.

    async fn orders_page(&self, page: u32, size: u32, source: Option<Source>) -> Result<OrderPage> {
        let source = source.map(Source::wire).unwrap_or("ALL");
        let raw = self
            .get(&format!(
                "/v1/edge/order/paged?page={page}&source={source}&size={size}"
            ))
            .await
            .map_err(needs_login("order history"))?;
        Ok(serde_json::from_value::<WireOrderPage>(raw)
            .context("parsing the order list")?
            .into_page())
    }

    /// Page until `max` orders are in hand or the history runs out. Pages are
    /// numbered from one here, unlike the search endpoint's.
    pub async fn orders(&self, max: u32, source: Option<Source>) -> Result<OrderPage> {
        let per_page = max.clamp(1, MAX_ORDERS_PER_PAGE);
        let mut orders = Vec::new();
        let mut page = 1u32;
        let mut total_pages = 1u32;
        let mut total = 0u32;

        while (orders.len() as u32) < max && page <= total_pages {
            let res = self.orders_page(page, per_page, source).await?;
            total = res.total;
            total_pages = res.total_pages.max(1);
            if res.orders.is_empty() {
                break;
            }
            let room = max as usize - orders.len();
            orders.extend(res.orders.into_iter().take(room));
            page += 1;
        }

        Ok(OrderPage {
            orders,
            total,
            total_pages,
        })
    }

    /// One order and its lines.
    pub async fn order(&self, id: &str, source: Source) -> Result<Order> {
        // A till receipt's id is a path, and the site sends it as a query
        // parameter with its slashes intact, so it is spliced in rather than
        // encoded. An online id is a single segment and goes in the path.
        let path = match source {
            Source::InStore => format!("/v1/edge/order/instore?orderId={id}"),
            Source::Online => format!("/v1/edge/order/{id}"),
        };
        let raw = self
            .get(&path)
            .await
            .map_err(needs_login("order history"))?;
        serde_json::from_value::<WireOrderDetail>(raw)
            .context("parsing the order")?
            .into_order()
            .with_context(|| format!("no {} order {id} on this account", source.label()))
    }

    /// What this account has bought before, most recently bought first. The
    /// site's "buy it again"; it spans both kinds of order.
    pub async fn previous_purchases(&self, max: u32, exclude_cart: bool) -> Result<Vec<OrderLine>> {
        let body = serde_json::json!({
            "excludeCart": exclude_cart,
            "maximumResults": max,
        });
        let raw = self
            .post("/v1/edge/order/previousPurchases", &body)
            .await
            .map_err(needs_login("previous purchases"))?;
        Ok(serde_json::from_value::<WirePreviousPurchases>(raw)
            .context("parsing previous purchases")?
            .into_lines())
    }

    /// Every store the banner runs. Prices and stock are per store, which is
    /// why one has to be chosen before searching.
    pub async fn stores(&self) -> Result<Vec<Store>> {
        let raw = self.get("/v1/edge/store").await?;
        // Seen both as a bare array and wrapped in `{ "stores": [...] }`.
        let items = match raw {
            serde_json::Value::Array(items) => items,
            serde_json::Value::Object(ref map) => match map.get("stores") {
                Some(serde_json::Value::Array(items)) => items.clone(),
                _ => bail!("store list response had no 'stores' array"),
            },
            _ => bail!("unexpected store list response"),
        };

        let mut stores = Vec::new();
        for item in items {
            let wire: WireStore = match serde_json::from_value(item) {
                Ok(w) => w,
                Err(_) => continue,
            };
            let Some(id) = wire.id else { continue };
            stores.push(Store {
                id,
                name: wire.name.unwrap_or_else(|| "(unnamed)".into()),
                banner: self.banner.id(),
                region: wire.region,
                address: wire.address.and_then(|a| a.as_str().map(str::to_string)),
            });
        }
        Ok(stores)
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
        let raw = self
            .post("/v1/edge/search/paginated/products", &body)
            .await?;
        serde_json::from_value(raw).context("parsing the product search response")
    }

    /// Page until `max` products are in hand or the results run out.
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
                (Some(t), Some(v)) => Some(format!("{t} for {}", crate::domain::dollars(v))),
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
            banner: self.banner.id(),
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

pub struct SearchResult {
    pub products: Vec<Product>,
    pub total_available: u32,
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

/// On the account-scoped endpoints a 401 almost always means "not logged in"
/// rather than "token expired", so say the useful thing. `what` names the thing
/// that needed the account.
fn needs_login(what: &'static str) -> impl Fn(anyhow::Error) -> anyhow::Error {
    move |e| {
        let text = format!("{e:#}");
        if text.contains("401") || text.contains("403") {
            return e.context(format!("{what} needs an account: run `fsnz auth login`"));
        }
        // The cart is bound to a store separately from the one searches price
        // against. A token that is not banner-scoped reaches the cart endpoints
        // but has no cart of its own, which shows up here. Only the cart says
        // this, so only the cart ever sees the hint.
        if text.contains("Store is not defined") {
            return e.context(
                "this account's cart has no store bound to it. That store is \
                 separate from the one `fsnz store set` prices against, and \
                 binding it needs a banner-scoped token: check `fsnz auth status` \
                 reports MNW or PNS, not NAT.",
            );
        }
        e
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}...")
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
        assert_eq!(
            filters("s1", true, Some("Bakery")),
            "stores:s1 AND category0NI:\"Bakery\" AND onPromotion:s1"
        );
    }

    #[test]
    fn blank_departments_are_ignored() {
        assert_eq!(filters("s1", false, Some("   ")), "stores:s1");
    }

    #[test]
    fn truncate_leaves_short_strings_alone() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abcdef", 3), "abc...");
    }
}
