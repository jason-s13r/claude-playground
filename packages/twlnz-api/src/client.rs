//! The storefront, and everything reached through it.

use std::sync::Mutex;

use net_kit::wreq;

use crate::cart;
use crate::domain::{Cart, Category, Island, ProductDetail, Store, StoreStock};
use crate::endpoints::Endpoints;
use crate::error::{Error, Result};
use crate::extract;
use crate::listing::{self, Facet, Listing, Query, PAGE_SIZE};
use crate::product::{Action, Pdp};
use crate::session::Session;
use crate::stores;
use crate::wire;

/// A page older than this is re-fetched before its tokens are spent.
///
/// The real expiry is not published. Five minutes is well inside the window
/// observed to work and costs one request when it is wrong, which is cheaper
/// than a refused write.
const TOKEN_MAX_AGE_SECS: u64 = 300;

/// What a client needs to sign itself in again when its session lapses.
///
/// Unlike Woolworths, this is a genuine renewal rather than a whole flow
/// re-walked out of necessity -- but it still takes a password, because the
/// form is what mints the cookie.
pub struct Reauth {
    pub email: String,
    pub password: net_kit::password::Source,
    pub secrets: net_kit::Secrets,
}

pub struct Client {
    http: wreq::Client,
    endpoints: Endpoints,
    /// Replaced in place by [`Client::renew`], so one command's later calls use
    /// the session its earlier ones bought.
    session: Mutex<Session>,
    reauth: Option<Reauth>,
    /// Which island stock is quoted for. Session state rather than a per-query
    /// choice: it changes what a listing *contains*, and the site keeps it on
    /// the shopper rather than on the request.
    island: Option<Island>,
    debug: bool,
}

impl Client {
    pub fn new(http: wreq::Client, endpoints: Endpoints, session: Session) -> Client {
        Client {
            http,
            endpoints,
            session: Mutex::new(session),
            reauth: None,
            island: None,
            debug: false,
        }
    }

    pub fn with_reauth(mut self, reauth: Option<Reauth>) -> Client {
        self.reauth = reauth;
        self
    }

    pub fn with_island(mut self, island: Option<Island>) -> Client {
        self.island = island;
        self
    }

    pub fn with_debug(mut self, debug: bool) -> Client {
        self.debug = debug;
        self
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    pub fn session(&self) -> Session {
        self.session.lock().expect("session lock").clone()
    }

    pub fn island(&self) -> Option<Island> {
        self.island
    }

    // ---- transport ----

    fn trace(&self, message: &str) {
        if self.debug {
            eprintln!("twlnz-api: {message}");
        }
    }

    /// One GET, with the session's cookies, keeping whatever it sets.
    ///
    /// Returns the final URL as well as the body, because a keyword search can
    /// redirect into a category and the caller has to know that it did.
    async fn get(&self, url: &str, params: &[(String, String)]) -> Result<(String, String)> {
        // Built rather than handed to `wreq`: this crate encodes only what it
        // composes itself, so a pre-signed URL can never be round-tripped.
        let target = if params.is_empty() {
            url.to_string()
        } else {
            format!("{url}?{}", crate::endpoints::query_string(params))
        };
        let mut req = self.http.get(&target);
        if let Some(cookies) = self.session().header() {
            if let Ok(mut value) = wreq::header::HeaderValue::from_str(&cookies) {
                value.set_sensitive(true);
                req = req.header(wreq::header::COOKIE, value);
            }
        }
        // The storefront serves a different page to a request that did not come
        // from itself.
        req = req.header(wreq::header::REFERER, format!("{}/", self.endpoints.origin));

        let sent = req.send().await;
        let (headers, body) = net_kit::http::text("GET", &target, sent)
            .await
            .map_err(crate::error::from_http)?;
        self.session.lock().expect("session lock").absorb(&headers);
        // `wreq` follows the redirect, so the landing URL is what has to be
        // read back to see where a search ended up.
        let landed = headers
            .get("x-final-url")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(&target)
            .to_string();
        Ok((landed, body))
    }

    /// The two shapes of background request this storefront serves.
    ///
    /// They are not interchangeable, and sending the wrong one is answered with
    /// `Cross-Origin Request Blocked` rather than with anything that explains
    /// itself. The site's own scripts use both, so this models both rather than
    /// picking one and hoping.
    async fn xhr(&self, what: &'static str, url: &str, shape: Xhr) -> Result<String> {
        let absolute = self.endpoints.absolute(url);
        // The page a browser would have been standing on. For anything about a
        // product that is the product page, because that is where the token in
        // the URL was minted.
        let referer = match pid_of(url) {
            Some(pid) => self.endpoints.product_page("p", &pid),
            None => format!("{}/", self.endpoints.origin),
        };

        let mut req = self
            .http
            .get(&absolute)
            .header(wreq::header::REFERER, referer)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-site", "same-origin");
        req = match shape {
            Xhr::Fetch => req
                .header(wreq::header::ACCEPT, "application/json")
                .header("x-requested-with", "fetch")
                .header("sec-fetch-mode", "same-origin"),
            Xhr::Legacy => req
                .header(wreq::header::ACCEPT, "text/html,application/json;q=0.1")
                .header("x-requested-with", "XMLHttpRequest")
                .header("sec-fetch-mode", "cors"),
        };
        if let Some(cookies) = self.session().header() {
            if let Ok(mut value) = wreq::header::HeaderValue::from_str(&cookies) {
                value.set_sensitive(true);
                req = req.header(wreq::header::COOKIE, value);
            }
        }

        let sent = req.send().await;
        let (headers, body) = net_kit::http::text("GET", &absolute, sent)
            .await
            .map_err(crate::error::from_http)?;
        self.session.lock().expect("session lock").absorb(&headers);
        self.trace(&format!("read {what}"));
        Ok(body)
    }

    /// A GET against a pre-signed action URL, answered as text.
    ///
    /// The URL is used exactly as the page wrote it -- no re-parsing and no
    /// re-encoding, either of which would break the `verify` HMAC.
    ///
    /// The headers are not decoration. These endpoints answer
    /// `{"error":true,"errorMessage":"Cross-Origin Request Blocked"}` with a
    /// 403 unless the request looks like one the page itself made, so a request
    /// that is otherwise perfect -- right token, right cookies -- is refused for
    /// its headers alone.
    ///
    /// **The `Sec-Fetch-*` trio is the load-bearing part**, and it is the one
    /// the emulation cannot supply: `wreq` sends the values for a navigation,
    /// which is exactly what this check rejects. The message means what it says;
    /// it is not a disguised complaint about the token.
    async fn action_text(&self, what: &'static str, url: &str) -> Result<String> {
        self.xhr(what, url, Xhr::Fetch).await
    }

    /// The same, read as a checked action envelope.
    async fn action(&self, action: &'static str, url: &str) -> Result<serde_json::Value> {
        let body = self.action_text(action, url).await?;
        cart::checked(action, &body)
    }

    /// A form POST, in the same `fetch()` shape as [`Client::xhr`].
    ///
    /// Not every write is a signed GET. The cart and wishlist *adds* post a
    /// body -- `pid`, `quantity`, and a context saying which page it came from
    /// -- while keeping the `verify` token in the query string. Calling one of
    /// those as a GET is answered with a 500 that explains nothing.
    ///
    /// The URL is used exactly as given, because it carries that token.
    async fn post_form(
        &self,
        action: &'static str,
        url: &str,
        form: &[(&str, &str)],
    ) -> Result<serde_json::Value> {
        let absolute = self.endpoints.absolute(url);
        let referer = match pid_of(url) {
            Some(pid) => self.endpoints.product_page("p", &pid),
            None => self.endpoints.cart_page(),
        };
        let mut req = self
            .http
            .post(&absolute)
            .header(wreq::header::ACCEPT, "application/json")
            .header("x-requested-with", "fetch")
            .header(wreq::header::REFERER, referer)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "same-origin")
            .header("sec-fetch-site", "same-origin");
        if let Some(cookies) = self.session().header() {
            if let Ok(mut value) = wreq::header::HeaderValue::from_str(&cookies) {
                value.set_sensitive(true);
                req = req.header(wreq::header::COOKIE, value);
            }
        }
        let sent = req.form(form).send().await;
        let (headers, body) = net_kit::http::text("POST", &absolute, sent)
            .await
            .map_err(crate::error::from_http)?;
        self.session.lock().expect("session lock").absorb(&headers);
        self.trace(&format!("posted {action}"));
        cart::checked(action, &body)
    }

    /// Sign in again and keep what it produces.
    pub async fn renew(&self) -> Result<Session> {
        let reauth = self.reauth.as_ref().ok_or(Error::NotSignedIn)?;
        let password = reauth.password.password().await?;
        let trace: crate::auth::Trace<'_> = &|m| self.trace(m);
        let session =
            crate::auth::login(&self.http, &self.endpoints, &reauth.email, &password, trace)
                .await?;
        crate::session::StoredSession::of(&session, Some(reauth.email.clone()))
            .save(&reauth.secrets)?;
        *self.session.lock().expect("session lock") = session.clone();
        Ok(session)
    }

    /// Make sure the session speaks for an account before an account-only call.
    ///
    /// The access token is good for half an hour, so this is not a rare path:
    /// a client with a password signs itself in again rather than failing a
    /// command that was about to work.
    async fn require_account(&self) -> Result<()> {
        let session = self.session();
        if session.account() && !session.lapsed() {
            return Ok(());
        }
        if self.reauth.is_some() {
            self.renew().await?;
            return Ok(());
        }
        Err(if session.account() {
            Error::SessionExpired
        } else {
            Error::NotSignedIn
        })
    }

    // ---- listing ----

    /// One window of a listing.
    ///
    /// `start` is an offset into the whole result set and random access works,
    /// so a caller can jump rather than walk.
    pub async fn page(
        &self,
        query: &Query,
        start: u32,
        size: u32,
        sort: Option<&str>,
        facets: &[Facet],
    ) -> Result<Listing> {
        let params = listing::params(query, start, size, sort, self.island, facets);
        let (landed, body) = self.get(&self.endpoints.updategrid(), &params).await?;
        Ok(Listing {
            products: extract::tiles(&body),
            total: extract::listing_total(&body),
            category: listing::category_from_url(&landed),
        })
    }

    /// Page until `max` products are in hand or the results run out.
    ///
    /// Two stopping conditions rather than one: the total when the page
    /// reported it, and an empty window either way -- a listing fragment does
    /// not always carry a header, and paging past the end is otherwise silent.
    pub async fn search(
        &self,
        query: &Query,
        max: u32,
        sort: Option<&str>,
        facets: &[Facet],
    ) -> Result<Listing> {
        let mut query = query.clone();
        let mut products = Vec::new();
        let mut total = None;
        let mut category = None;
        let mut start = 0u32;

        while (products.len() as u32) < max {
            let size = PAGE_SIZE.min(max - products.len() as u32).max(1);
            let page = self.page(&query, start, size, sort, facets).await?;
            total = page.total.or(total);
            let got = page.products.len() as u32;
            products.extend(page.products);

            if let Some(landed) = page.category {
                // A keyword that resolved to a category: page the category from
                // here, because `q=` and `cgid=` are not interchangeable once
                // the site has decided which it is.
                if category.is_none() && matches!(query, Query::Keyword(_)) {
                    self.trace(&format!("the search resolved to the category {landed}"));
                    query = Query::Category(landed.clone());
                }
                category.get_or_insert(landed);
            }

            // A window shorter than the one asked for is the end of the
            // results. This is the load-bearing stop, not the total: the total
            // counts the whole listing while the window is what was actually
            // served, and a listing fragment does not always carry a header to
            // read a total from at all.
            if got < size {
                break;
            }
            if total.is_some_and(|t| products.len() as u32 >= t) {
                break;
            }
            start += got;
        }

        products.truncate(max as usize);
        Ok(Listing {
            products,
            total,
            category,
        })
    }

    /// What the typeahead offers, which is the cheapest way to turn a phrase
    /// into product ids.
    ///
    /// The one endpoint still on the older `XMLHttpRequest` shape -- it answers
    /// HTML and wants `Sec-Fetch-Mode: cors` rather than `same-origin`.
    pub async fn suggest(&self, text: &str) -> Result<Vec<(String, String)>> {
        let url = format!(
            "{}?{}",
            self.endpoints.suggestions(),
            crate::endpoints::query_string(&[("q".to_string(), text.to_string())])
        );
        let body = self.xhr("the suggestions", &url, Xhr::Legacy).await?;
        Ok(extract::suggestions(&body))
    }

    // ---- products ----

    /// Read a product page, which is also what mints the tokens every write
    /// needs.
    pub async fn pdp(&self, pid: &str) -> Result<Pdp> {
        let url = self.endpoints.product_page("p", pid);
        let (_, body) = self.get(&url, &[]).await?;
        Pdp::parse(pid, &body)
    }

    /// The full detail, which needs the variation endpoint as well as the page:
    /// the axes, the per-channel availability and the order limits are only
    /// there.
    pub async fn product(&self, pid: &str) -> Result<ProductDetail> {
        let pdp = self.pdp(pid).await?;
        match pdp.action(Action::Variation) {
            Ok(url) => {
                let value = self.action("the product detail", url).await?;
                let parsed: wire::VariationResponse = serde_json::from_value(value)
                    .map_err(|e| Error::decode("parsing the product detail", e))?;
                let mut detail = parsed.product.into_detail();
                // The page knows things the JSON does not: the category path
                // and the canonical description.
                if detail.product.category.is_none() {
                    detail.product.category = pdp.detail.product.category.clone();
                }
                if detail.description.is_none() {
                    detail.description = pdp.detail.description.clone();
                }
                Ok(detail)
            }
            // A product with no variations has no variation endpoint, and the
            // page alone is the whole answer.
            Err(_) => Ok(pdp.detail),
        }
    }

    /// Choose a value on one axis, which answers with the variant it resolves
    /// to -- its own price, stock and id.
    pub async fn select(&self, pdp: &Pdp, axis: &str, value: &str) -> Result<ProductDetail> {
        let url = pdp.select(axis, value)?;
        let json = self.action("the variation", &url).await?;
        let parsed: wire::VariationResponse =
            serde_json::from_value(json).map_err(|e| Error::decode("parsing the variation", e))?;
        Ok(parsed.product.into_detail())
    }

    /// Per-store stock for a product, optionally narrowed to one region.
    ///
    /// Narrowing is a **third** request, not a parameter on the second. The
    /// stock modal is rendered with one pre-signed URL per region, each
    /// carrying its own `verify` token minted at that moment; the product
    /// page's token does not authorise the regional endpoint, and a URL built
    /// by hand is refused however well formed it is. So the walk is: product
    /// page, then the modal, then the region's own link out of it.
    pub async fn stock(&self, pdp: &Pdp, region: Option<&str>) -> Result<Vec<StoreStock>> {
        let url = pdp.action(Action::StoreStock)?;
        let body = self.action_text("the store stock", url).await?;

        let Some(region) = region else {
            return stores::parse_stock(&body);
        };

        // The modal's markup is inside a JSON field, so the links have to come
        // out of that rather than off the response.
        let value: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| Error::decode("parsing the store stock", e))?;
        let markup = ["modalTemplate", "stores"]
            .iter()
            .find_map(|k| value.get(*k).and_then(|v| v.as_str()))
            .ok_or_else(|| Error::not_in_page("the store availability markup"))?;

        let links = extract::region_links(markup);
        let link = links.get(region).ok_or_else(|| Error::NotInPage {
            what: format!("a stock link for {region}"),
            detail: match links.is_empty() {
                true => ", and the page offered none at all".into(),
                false => format!(
                    ", which offers {}",
                    links.keys().cloned().collect::<Vec<_>>().join(", ")
                ),
            },
        })?;

        let body = self.action_text("the regional store stock", link).await?;
        stores::parse_stock(&body)
    }

    // ---- cart and wishlist ----

    /// What is in the basket.
    ///
    /// An XHR endpoint despite carrying no token, so it goes the same way the
    /// writes do. Fetched as a page it is refused, which is a confusing thing
    /// for a read to do.
    pub async fn cart(&self) -> Result<Cart> {
        let body = self
            .action_text("the cart", &self.endpoints.minicart())
            .await?;
        cart::cart_from(cart::checked("the cart", &body)?)
    }

    /// Add to the cart, re-reading the page once if its token has gone stale.
    ///
    /// The retry is the reason a `Pdp` is taken rather than a pid: it is
    /// cheap and certain, because the only thing that can have expired is the
    /// token this fetched.
    pub async fn add_to_cart(&self, pdp: &Pdp, quantity: u32) -> Result<Cart> {
        let mut pdp = std::borrow::Cow::Borrowed(pdp);
        if pdp.stale(TOKEN_MAX_AGE_SECS) {
            self.trace("the page token has aged out, re-reading the product page");
            pdp = std::borrow::Cow::Owned(self.pdp(&pdp.pid).await?);
        }
        let quantity = quantity.max(1).to_string();
        let url = pdp.action(Action::AddToCart)?.to_string();
        match self.add(&url, &pdp.pid, &quantity).await {
            Err(e) if e.is_stale_token() => {
                self.trace("the token was refused, re-reading the product page");
                let fresh = self.pdp(&pdp.pid).await?;
                let url = fresh.action(Action::AddToCart)?.to_string();
                cart::cart_from(self.add(&url, &fresh.pid, &quantity).await?)
            }
            other => cart::cart_from(other?),
        }
    }

    async fn add(&self, url: &str, pid: &str, quantity: &str) -> Result<serde_json::Value> {
        self.post_form(
            "the cart add",
            url,
            &[
                ("pid", pid),
                ("quantity", quantity),
                // Where the add came from. This crate only ever adds from a
                // product page, which is what the site sends there.
                ("context", "PDP"),
                // Cloudflare Turnstile. The site posts this empty or literally
                // `undefined` on a request it has no challenge for, and both
                // are accepted -- so the field is sent, empty, rather than
                // omitted: a missing field is a different shape from an
                // unanswered challenge.
                ("cf-turnstile-response", ""),
            ],
        )
        .await
    }

    /// Set a line to an exact quantity.
    ///
    /// By product id, with no line id: that is what the site's own request
    /// sends, and it is also the only handle a person has -- the line uuid is
    /// something they never see.
    pub async fn set_quantity(&self, pid: &str, quantity: u32) -> Result<Cart> {
        let url = format!(
            "{}?pid={pid}&quantity={quantity}",
            self.endpoints.controller("Cart-UpdateQuantity")
        );
        cart::cart_from(self.action("the quantity change", &url).await?)
    }

    /// Take a line out.
    ///
    /// Takes the line's `pli_uuid`, not its `uuid`: the site reports two ids
    /// per line and this controller accepts only the second. Setting the
    /// quantity to zero is *not* an alternative -- the site accepts it, reports
    /// success, and leaves the line where it was.
    pub async fn remove_line(&self, line: &crate::CartLine) -> Result<Cart> {
        let uuid = line.pli_uuid.as_deref().unwrap_or(&line.uuid);
        let url = format!(
            "{}?pid={}&uuid={uuid}",
            self.endpoints.controller("Cart-RemoveProductLineItem"),
            line.id
        );
        cart::cart_from(self.action("the removal", &url).await?)
    }

    /// The wishlist belongs to a person, so this is the first call that
    /// genuinely needs an account.
    pub async fn add_to_wishlist(&self, pdp: &Pdp) -> Result<()> {
        self.require_account().await?;
        let url = pdp.action(Action::AddToWishlist)?.to_string();
        self.post_form(
            "the wishlist add",
            &url,
            &[("pid", &pdp.pid), ("quantity", "1")],
        )
        .await?;
        Ok(())
    }

    // ---- stores ----

    /// Every store in one region. There is no call that lists them all, so a
    /// whole-country list is one request per region.
    pub async fn stores(&self, region: &str) -> Result<Vec<Store>> {
        let code = stores::region(region).ok_or_else(|| Error::NoSuchStore(region.to_string()))?;
        let params = vec![("region".to_string(), code.to_string())];
        let (_, body) = self
            .get(&self.endpoints.controller("Stores-FindStores"), &params)
            .await?;
        stores::parse(&body)
    }

    /// Bind the cart to a store for click and collect.
    ///
    /// A form POST from the cart page, not a signed GET -- calling it as one is
    /// answered with a 500 that explains nothing. Both fields carry the same
    /// id: the site sends `preferredStoreId` alongside `storeId`, and setting
    /// only one is not what it does.
    ///
    /// **Needs a basket.** Against an empty cart this answers 500 with
    /// `redirectUrl: "/cart"`, which is the site saying there is nothing to
    /// bind a collection point to. So this belongs to checking out; a caller
    /// that only wants to remember a store should keep it locally instead.
    pub async fn set_store(&self, store_id: &str) -> Result<()> {
        self.post_form(
            "the store selection",
            &self.endpoints.controller("Cart-SelectStore"),
            &[("preferredStoreId", store_id), ("storeId", store_id)],
        )
        .await?;
        Ok(())
    }

    // ---- taxonomy ----

    /// The department tree, one level at a time.
    ///
    /// `Category-GetMultipleNavigationHierarchy` answers `maxDepth=0` without
    /// children, so depth costs a request per level. The default of one level
    /// is what a `departments` listing needs; deeper is opt-in because it is
    /// several round trips.
    pub async fn categories(&self, roots: &[&str], depth: u32) -> Result<Vec<Category>> {
        let params = vec![
            ("cgids".to_string(), roots.join(",")),
            ("maxDepth".to_string(), depth.to_string()),
        ];
        let (_, body) = self
            .get(
                &self
                    .endpoints
                    .controller("Category-GetMultipleNavigationHierarchy"),
                &params,
            )
            .await?;
        let parsed: wire::CategoriesResponse = serde_json::from_str(&body)
            .map_err(|e| Error::decode("parsing the category tree", e))?;
        Ok(parsed
            .categories
            .into_iter()
            .filter_map(wire::WireCategory::into_category)
            .collect())
    }
}

/// Which background-request shape an endpoint expects.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Xhr {
    /// `fetch()`: the cart, the wishlist, product actions, the minicart.
    Fetch,
    /// `XMLHttpRequest`: the search typeahead, which predates the rest.
    Legacy,
}

/// The product an action URL is about, from whichever parameter names it.
///
/// `pid` on most controllers and `productId` on the regional stock one -- the
/// same thing under two names, because they are different SFCC controllers.
fn pid_of(url: &str) -> Option<String> {
    let query = url.split('?').nth(1)?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        matches!(key, "pid" | "productId").then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_product_an_action_is_about_is_found_under_either_name() {
        // It becomes the `Referer`, and these controllers refuse a request whose
        // headers do not place it on the site.
        assert_eq!(
            pid_of("/cart/add-product?pid=R1&verify=1-a").as_deref(),
            Some("R1")
        );
        assert_eq!(
            pid_of("/x/Stores-PDPStoreAvailability?productId=R2&region=NZ-CAN").as_deref(),
            Some("R2")
        );
        assert_eq!(pid_of("/cart/minicart"), None);
    }
}
