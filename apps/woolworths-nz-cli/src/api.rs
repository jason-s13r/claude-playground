//! The Woolworths NZ GraphQL API.
//!
//! One endpoint, `POST /api/graphql`, answers everything the website does:
//! search, browse, specials, stores, the cart and order history. It is
//! undocumented and reverse-engineered from the site's own traffic, so
//! everything is optional on the way in -- see [`wire`].
//!
//! Authorisation is entirely by cookie ([`crate::session`]). The guest token
//! covers products and stores; the cart and orders need an account.

pub mod gql;
mod wire;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::json;

use crate::domain::cart::{Cart, Change};
use crate::domain::category::Category;
use crate::domain::order::{Filter, OrderPage};
use crate::domain::{Product, Store};
use crate::session::Session;

/// The site's own default ordering.
pub const DEFAULT_SORT: &str = "RELEVANCE";

/// The orderings the site's own sort menu offers. Anything else is still passed
/// through -- an unfamiliar value should reach the API rather than be rejected
/// here -- but these are what `--sort` suggests.
pub const SORTS: [&str; 6] = [
    "RELEVANCE",
    "PRICE_LOW_HIGH",
    "PRICE_HIGH_LOW",
    "CUP_PRICE_LOW_HIGH",
    "FAVOURITES",
    "FREQUENCY",
];

/// The page size the site asks for. Nothing documents a ceiling, so this stays
/// at what is known to work and pages for the rest.
const MAX_PAGE_SIZE: u32 = 36;

/// How products are selected. All five are fields of one `CompositeSearchInput`,
/// which is why search, browse, specials and buy-again share a document.
#[derive(Clone, Debug)]
pub enum SearchBy {
    Keyword(String),
    Category(String),
    /// Everything currently on special. Carries no value of its own; the
    /// `SPECIALS` static filter is what selects it.
    Specials,
    /// What this account has bought before -- the site's "buy it again".
    BuyAgain,
}

impl SearchBy {
    fn field(&self) -> &'static str {
        match self {
            SearchBy::Keyword(_) => "byKeyword",
            SearchBy::Category(_) => "byCategoryKey",
            SearchBy::Specials => "byProductPromotionSpecials",
            SearchBy::BuyAgain => "byBuyAgain",
        }
    }

    fn value(&self) -> Option<&str> {
        match self {
            SearchBy::Keyword(v) | SearchBy::Category(v) => Some(v),
            SearchBy::Specials | SearchBy::BuyAgain => None,
        }
    }

    /// Whether this selection needs an account rather than a guest token.
    pub fn needs_account(&self) -> bool {
        matches!(self, SearchBy::BuyAgain)
    }

    /// A sensible default ordering. Buy-again is ranked by how often something
    /// was bought, which is the only ordering that makes sense for it.
    pub fn default_sort(&self) -> &'static str {
        match self {
            SearchBy::BuyAgain => "FREQUENCY",
            _ => DEFAULT_SORT,
        }
    }

    fn describe(&self) -> String {
        match self {
            SearchBy::Keyword(q) => format!("search for '{q}'"),
            SearchBy::Category(k) => format!("department '{k}'"),
            SearchBy::Specials => "specials".into(),
            SearchBy::BuyAgain => "previous purchases".into(),
        }
    }
}

pub struct SearchResult {
    pub products: Vec<Product>,
    pub total_available: u32,
}

/// Where the API lives. Overridable from the environment: these are
/// undocumented endpoints, so when Woolworths moves one a user should be able
/// to follow without waiting for a release. The tests point them at a local
/// mock server.
#[derive(Clone, Debug)]
pub struct Endpoints {
    /// The storefront, which mints the guest token and hosts the API.
    pub origin: String,
    /// Where the login flow is served, which is a different host in production.
    pub auth: String,
}

impl Endpoints {
    pub fn resolve() -> Endpoints {
        let pick = |key: &str, fallback: &str| {
            std::env::var(key)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| fallback.to_string())
                .trim_end_matches('/')
                .to_string()
        };
        Endpoints {
            origin: pick("WWNZ_ORIGIN", "https://www.woolworths.co.nz"),
            auth: pick("WWNZ_AUTH_ORIGIN", "https://auth.woolworths.co.nz"),
        }
    }

    pub fn graphql(&self) -> String {
        format!("{}/api/graphql", self.origin)
    }
}

pub struct Client {
    http: wreq::Client,
    endpoints: Endpoints,
    session: Session,
}

impl Client {
    pub fn new(http: wreq::Client, endpoints: Endpoints, session: Session) -> Client {
        Client {
            http,
            endpoints,
            session,
        }
    }

    /// Run one operation and hand back its `data`.
    ///
    /// GraphQL answers 200 with an `errors` array rather than an HTTP status,
    /// so the body has to be inspected either way.
    async fn call(
        &self,
        operation: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = format!("{}?op-name={operation}", self.endpoints.graphql());
        let body = json!({
            "operationName": operation,
            "variables": variables,
            "query": gql::document(operation),
        });

        let mut req = self
            .http
            .post(&url)
            .header(
                wreq::header::ACCEPT,
                "application/graphql-response+json,application/json;q=0.9",
            )
            .header(wreq::header::CONTENT_TYPE, "application/json")
            .header(wreq::header::ORIGIN, &self.endpoints.origin)
            .header(wreq::header::REFERER, format!("{}/", self.endpoints.origin))
            // The site sends the operation name as a header as well as in the
            // document. Cheap to match, and one less way to look unlike the
            // client this endpoint expects.
            .header("wnzx-operation-name", operation);

        if let Some(cookies) = self.session.header() {
            if let Ok(mut value) = wreq::header::HeaderValue::from_str(&cookies) {
                value.set_sensitive(true);
                req = req.header(wreq::header::COOKIE, value);
            }
        }

        let res = req
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;

        let status = res.status();
        let text = res.text().await.unwrap_or_default();

        if !status.is_success() {
            // A stored session that has lapsed is the common case here, and the
            // fix for it is not the fix for "you never signed in" -- so it is
            // named outright rather than left to `needs_login` to guess at.
            if status == wreq::StatusCode::UNAUTHORIZED && text.contains("session_expired") {
                bail!(
                    "the session has expired. Sign in again:\n  \
                     wwnz auth login --email you@example.com\n  \
                     wwnz auth import cookies.txt   (if the sign-in page refuses)"
                );
            }
            bail!("Woolworths API {status} for {operation}{}", detail(&text));
        }

        let parsed: serde_json::Value = serde_json::from_str(&text).with_context(|| {
            format!(
                "{operation} returned a body that is not JSON: {}",
                truncate(&text, 200)
            )
        })?;

        if let Some(errors) = parsed.get("errors").and_then(|e| e.as_array()) {
            if !errors.is_empty() {
                return Err(graphql_error(operation, errors));
            }
        }

        parsed
            .get("data")
            .filter(|d| !d.is_null())
            .cloned()
            .with_context(|| format!("{operation} returned no data"))
    }

    // ---- products ----

    async fn search_page(
        &self,
        by: &SearchBy,
        page: u32,
        page_size: u32,
        sort: &str,
        specials_only: bool,
    ) -> Result<wire::ProductPage> {
        let mut input = json!({
            "pageIndex": page,
            "pageSize": page_size,
            "facetFilters": [],
            "staticFilters": if specials_only { json!(["SPECIALS"]) } else { json!([]) },
            "sortBy": sort,
        });
        if let Some(value) = by.value() {
            input["value"] = json!(value);
        }

        let data = self
            .call(
                "ProductSearch",
                json!({ "searchInput": { by.field(): input } }),
            )
            .await
            .map_err(needs_login_for(by))?;

        let envelope: wire::SearchEnvelope =
            serde_json::from_value(data).context("parsing the product search response")?;
        envelope
            .my
            .and_then(|m| m.products)
            .context("the product search response carried no products")
    }

    /// Page until `max` products are in hand or the results run out.
    pub async fn search(
        &self,
        by: &SearchBy,
        max: u32,
        sort: &str,
        specials_only: bool,
    ) -> Result<SearchResult> {
        let page_size = max.clamp(1, MAX_PAGE_SIZE);
        let mut products: Vec<Product> = Vec::new();
        let mut page = 0u32;
        let mut total_pages = 1u32;
        let mut total_available = 0u32;

        while (products.len() as u32) < max && page < total_pages {
            let res = self
                .search_page(by, page, page_size, sort, specials_only)
                .await?;
            total_pages = res.total_pages.unwrap_or(page + 1);

            let mapped: Vec<Product> = res
                .results
                .into_iter()
                .filter_map(|r| match r {
                    wire::WireResult::ProductSummary(p) => {
                        p.into_product(&self.endpoints.origin, false)
                    }
                    // Ads are real products at real prices, marked so a reader
                    // can tell why they are at the top.
                    wire::WireResult::SponsoredProduct(p) => {
                        p.into_product(&self.endpoints.origin, true)
                    }
                    wire::WireResult::Other => None,
                })
                .collect();

            total_available = res
                .total_count
                .unwrap_or(products.len() as u32 + mapped.len() as u32);

            // A page can be all ad slots and no products, which is not the end
            // of the results -- only an empty page from the server is.
            if mapped.is_empty() && page + 1 >= total_pages {
                break;
            }

            let room = max as usize - products.len();
            products.extend(mapped.into_iter().take(room));
            page += 1;
        }

        Ok(SearchResult {
            products,
            total_available,
        })
    }

    /// The whole department tree.
    pub async fn categories(&self) -> Result<Category> {
        let data = self
            .call("GetAllCategories", json!({ "categoryKey": "" }))
            .await?;
        let envelope: wire::SearchEnvelope =
            serde_json::from_value(data).context("parsing the category tree")?;
        envelope
            .my
            .and_then(|m| m.categories)
            .and_then(wire::WireCategory::into_category)
            .context("the category response carried no categories")
    }

    // ---- stores ----

    /// Stores, by name. With no query this asks for every store the chain runs.
    pub async fn stores(&self, query: Option<&str>, max: u32) -> Result<Vec<Store>> {
        let input = json!({
            "search": query.unwrap_or(""),
            // With no search term the API only answers with anything at all
            // when it is told to list them all.
            "allStores": true,
            "filter": {
                "sortingMethod": "DISTANCE",
                "sortingOrder": "ASCENDING",
                "max": max,
            },
        });
        let data = self
            .call("SearchLocations", json!({ "input": input }))
            .await?;
        let envelope: wire::LocationsEnvelope =
            serde_json::from_value(data).context("parsing the store list")?;
        Ok(envelope
            .locations
            .map(|l| l.locations)
            .unwrap_or_default()
            .into_iter()
            .filter_map(wire::WireLocation::into_store)
            .collect())
    }

    /// Bind the cart to a store, which is what prices are then quoted against.
    ///
    /// This is a cart mutation rather than a saved preference because on this
    /// site the store *is* a property of the cart. It works for a guest too.
    pub async fn set_store(&self, store_id: &str) -> Result<Option<String>> {
        let data = self
            .call(
                "SetCartShoppingMode",
                json!({ "setCartShoppingModeInput": {
                    "pickupLocationId": store_id,
                    "shoppingMode": "Pickup",
                }}),
            )
            .await?;
        Ok(data
            .get("setCartShoppingMode")
            .and_then(|c| c.get("shoppingMode"))
            .and_then(|s| s.get("pickupLocation"))
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(str::to_string))
    }

    // ---- cart ----
    // The cart belongs to a session. A guest has one of their own, but it is
    // thrown away with the token, so these all want an account.

    pub async fn cart(&self) -> Result<Cart> {
        let data = self
            .call("CustomerCart", json!({}))
            .await
            .map_err(needs_login("the cart"))?;
        self.read_cart(data, "customerCart")
    }

    /// Apply quantity changes and return the cart as it stands afterwards.
    pub async fn cart_set(&self, changes: &[Change]) -> Result<Cart> {
        if changes.is_empty() {
            return self.cart().await;
        }
        let data = self
            .call(
                "SetCartLineItemQuantity",
                json!({ "input": { "cartLineItemQuantityUpdates": changes } }),
            )
            .await
            .map_err(needs_login("the cart"))?;
        self.read_cart(data, "setCartLineItemQuantity")
    }

    pub async fn cart_clear(&self) -> Result<Cart> {
        let data = self
            .call("ClearCart", json!({}))
            .await
            .map_err(needs_login("the cart"))?;
        self.read_cart(data, "clearCart")
    }

    fn read_cart(&self, data: serde_json::Value, field: &str) -> Result<Cart> {
        let raw = data
            .get(field)
            .filter(|v| !v.is_null())
            .with_context(|| format!("the response carried no {field}"))?;
        Ok(serde_json::from_value::<wire::WireCart>(raw.clone())
            .context("parsing the cart")?
            .into_cart())
    }

    // ---- orders ----

    async fn orders_page(&self, page: u32, size: u32, filter: Filter) -> Result<OrderPage> {
        let data = self
            .call(
                "Orders",
                json!({ "input": {
                    "pageIndex": page,
                    "pageSize": size,
                    "inclusiveFilter": filter.wire(),
                }}),
            )
            .await
            .map_err(needs_login("order history"))?;
        let envelope: wire::OrdersEnvelope =
            serde_json::from_value(data).context("parsing the order list")?;
        Ok(envelope
            .orders
            .context("the order response carried no orders")?
            .into_page())
    }

    /// Page until `max` orders are in hand or the history runs out.
    pub async fn orders(&self, max: u32, filter: Filter) -> Result<OrderPage> {
        let per_page = max.clamp(1, MAX_PAGE_SIZE);
        let mut orders = Vec::new();
        let mut page = 0u32;
        let mut total_pages = 1u32;
        let mut total = 0u32;

        while (orders.len() as u32) < max && page < total_pages {
            let res = self.orders_page(page, per_page, filter).await?;
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
}

/// Turn a GraphQL `errors` array into one error.
///
/// The API signals "not logged in" as an `AUTH_NOT_AUTHENTICATED` extension on
/// a 200, so this is where an expired session is actually noticed.
fn graphql_error(operation: &str, errors: &[serde_json::Value]) -> anyhow::Error {
    let unauthenticated = errors.iter().any(|e| {
        e.get("extensions")
            .and_then(|x| x.get("code"))
            .and_then(|c| c.as_str())
            .is_some_and(|c| c.contains("AUTH_NOT_AUTHENTICATED") || c.contains("UNAUTHENTICATED"))
    });

    let messages: Vec<String> = errors
        .iter()
        .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
        .map(str::to_string)
        .collect();
    let joined = if messages.is_empty() {
        truncate(&serde_json::to_string(errors).unwrap_or_default(), 300)
    } else {
        messages.join("; ")
    };

    let e = anyhow!("{operation} failed: {joined}");
    if unauthenticated {
        // Marked so `needs_login` can recognise it whichever call raised it.
        return e.context("not signed in");
    }
    e
}

/// Say the useful thing when an account-scoped call is made without an account.
fn needs_login(what: &'static str) -> impl Fn(anyhow::Error) -> anyhow::Error {
    move |e| {
        let text = format!("{e:#}");
        if text.contains("has expired") {
            return e;
        }
        if text.contains("not signed in") || text.contains("401") || text.contains("403") {
            return e.context(format!("{what} needs an account: run `wwnz auth login`"));
        }
        e
    }
}

/// The same, for a product search that happens to be account-scoped.
fn needs_login_for(by: &SearchBy) -> impl Fn(anyhow::Error) -> anyhow::Error {
    let what = by.describe();
    let scoped = by.needs_account();
    move |e| {
        let text = format!("{e:#}");
        if text.contains("has expired") {
            return e;
        }
        if scoped && (text.contains("not signed in") || text.contains("401")) {
            return e.context(format!("{what} needs an account: run `wwnz auth login`"));
        }
        e
    }
}

fn detail(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        String::new()
    } else {
        format!(": {}", truncate(body, 300))
    }
}

fn truncate(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((cut, _)) => format!("{}...", &s[..cut]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_way_of_selecting_products_names_its_own_input_field() {
        assert_eq!(SearchBy::Keyword("milk".into()).field(), "byKeyword");
        assert_eq!(SearchBy::Category("9-ABC".into()).field(), "byCategoryKey");
        assert_eq!(SearchBy::Specials.field(), "byProductPromotionSpecials");
        assert_eq!(SearchBy::BuyAgain.field(), "byBuyAgain");
    }

    #[test]
    fn only_the_valued_selections_carry_a_value() {
        assert_eq!(SearchBy::Keyword("milk".into()).value(), Some("milk"));
        assert_eq!(SearchBy::Specials.value(), None);
        assert_eq!(SearchBy::BuyAgain.value(), None);
    }

    #[test]
    fn buy_again_is_the_only_account_scoped_selection() {
        assert!(SearchBy::BuyAgain.needs_account());
        assert!(!SearchBy::Keyword("milk".into()).needs_account());
        assert!(!SearchBy::Specials.needs_account());
        // Frequency is the only ordering that means anything for buy-again.
        assert_eq!(SearchBy::BuyAgain.default_sort(), "FREQUENCY");
        assert_eq!(SearchBy::Specials.default_sort(), DEFAULT_SORT);
    }

    #[test]
    fn an_unauthenticated_graphql_error_is_marked_as_one() {
        let errors = vec![serde_json::json!({
            "message": "The current user is not authorized to access this resource.",
            "extensions": { "code": "AUTH_NOT_AUTHENTICATED" },
        })];
        let e = graphql_error("Orders", &errors);
        let text = format!("{e:#}");
        assert!(text.contains("not signed in"), "{text}");

        // And the hint that reads off that marker reaches the user.
        let hinted = needs_login("order history")(e);
        assert!(format!("{hinted:#}").contains("wwnz auth login"));
    }

    #[test]
    fn an_expired_session_is_not_reported_as_never_having_signed_in() {
        // The two have different fixes, so the expiry message must survive
        // being passed through the account-scoped hint.
        let expired = anyhow!("the session has expired. Sign in again");
        let passed = needs_login("order history")(expired);
        let text = format!("{passed:#}");
        assert!(text.contains("has expired"), "{text}");
        assert!(!text.contains("needs an account"), "{text}");
    }

    #[test]
    fn an_ordinary_graphql_error_is_not_mistaken_for_a_login_problem() {
        let errors = vec![serde_json::json!({ "message": "Unknown fragment \"CartFields\"." })];
        let e = graphql_error("CustomerCart", &errors);
        assert!(format!("{e:#}").contains("Unknown fragment"));
        let passed = needs_login("the cart")(e);
        assert!(!format!("{passed:#}").contains("auth login"));
    }

    #[test]
    fn errors_with_no_message_still_say_something() {
        let errors = vec![serde_json::json!({ "extensions": { "code": "BAD" } })];
        assert!(format!("{:#}", graphql_error("X", &errors)).contains("BAD"));
    }

    #[test]
    fn endpoints_can_be_pointed_somewhere_else() {
        std::env::set_var("WWNZ_ORIGIN", "http://localhost:1234/");
        let e = Endpoints::resolve();
        // The trailing slash is dropped so paths concatenate cleanly.
        assert_eq!(e.origin, "http://localhost:1234");
        assert_eq!(e.graphql(), "http://localhost:1234/api/graphql");
        std::env::remove_var("WWNZ_ORIGIN");
    }

    #[test]
    fn truncation_does_not_split_a_character() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abcdef", 3), "abc...");
        // Multi-byte, which byte slicing would panic on.
        assert_eq!(truncate("héllo wörld", 4), "héll...");
    }
}
