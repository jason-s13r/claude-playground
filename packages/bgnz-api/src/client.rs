//! One client over three surfaces.
//!
//! They need different things, and the client's job is mostly to keep that
//! straight:
//!
//! | | needs a token | needs the `store` header |
//! |---|---|---|
//! | Klevu (search, browse) | no | no -- the API key carries the fascia |
//! | GraphQL catalogue, stores, stock | no | **yes** |
//! | cart, wishlist, orders, loyalty | **yes** | **yes** |
//!
//! The `store` header is the one that bites. Magento does not reject a request
//! without it -- it serves the default store, Briscoes -- so a forgotten header
//! on a Rebel Sport command answers with homeware and no error at all. It is
//! set in exactly one place below for that reason.
//!
//! Renewal is cheap, and deliberately so: a lapsed Magento token is one
//! unguarded Gigya call and one mutation away from a fresh one, with no
//! password and no browser. The browser is only needed when the stored Gigya
//! login itself is refused, and that is reported rather than worked around --
//! see [`crate::auth`].

use std::sync::Mutex;

use net_kit::wreq;
use serde::de::DeserializeOwned;

use crate::banner::{Banner, Endpoints};
use crate::domain::{
    Cart, Category, Customer, Fulfilment, GigyaConfig, Listing, Loyalty, OrderDetail, OrderPage,
    ProductDetail, Receipt, ReceiptPage, Stock, Store, Storefront, Wishlist,
};
use crate::error::{Error, Result};
use crate::search::Query;
use crate::session::Session;
use crate::{auth, gql, search, stock, wire};

/// Where a renewed session is filed.
///
/// Kept apart from the credentials themselves: a client that can renew and a
/// client that can *remember* it renewed are different capabilities, and a
/// renewal held only in memory means the next command starts from a token it
/// already knows is stale.
pub struct SessionStore {
    pub secrets: net_kit::Secrets,
}

pub struct Client {
    http: wreq::Client,
    endpoints: Endpoints,
    banner: Banner,
    /// Replaced in place by [`Client::renew`], so one command's later calls use
    /// the token its earlier ones bought.
    session: Mutex<Session>,
    /// `storeConfig` answers, fetched at most once per client.
    storefront: Mutex<Option<Storefront>>,
    gigya: Mutex<Option<GigyaConfig>>,
    store: Option<SessionStore>,
    debug: bool,
}

/// Whether a call needs an account behind it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Auth {
    None,
    Required,
}

impl Client {
    pub fn new(
        http: wreq::Client,
        endpoints: Endpoints,
        banner: Banner,
        session: Session,
    ) -> Client {
        Client {
            http,
            endpoints,
            banner,
            session: Mutex::new(session),
            storefront: Mutex::new(None),
            gigya: Mutex::new(None),
            store: None,
            debug: false,
        }
    }

    pub fn with_session_store(mut self, store: Option<SessionStore>) -> Client {
        self.store = store;
        self
    }

    pub fn with_debug(mut self, debug: bool) -> Client {
        self.debug = debug;
        self
    }

    pub fn banner(&self) -> Banner {
        self.banner
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    pub fn session(&self) -> Session {
        self.session.lock().expect("session lock").clone()
    }

    // -------------------------------------------------------- the storefront

    /// What the storefront publishes about itself, fetched once per client.
    pub async fn storefront(&self) -> Result<Storefront> {
        if let Some(cached) = self.storefront.lock().expect("storefront lock").clone() {
            return Ok(cached);
        }
        let answer: wire::StoreConfigEnvelope = self
            .gql(
                "KlevuData",
                gql::KLEVU_DATA,
                serde_json::json!({}),
                Auth::None,
            )
            .await?;
        let config = answer.store_config.unwrap_or_default();
        let storefront = Storefront {
            store_code: config
                .store_code
                .unwrap_or_else(|| self.banner.store_code().to_string()),
            klevu_url: config.klevu_search_url,
            klevu_key: config.klevu_search_js_api_key,
        };
        *self.storefront.lock().expect("storefront lock") = Some(storefront.clone());
        Ok(storefront)
    }

    /// Which SAP Customer Data Cloud site this fascia authenticates against.
    pub async fn gigya_config(&self) -> Result<GigyaConfig> {
        if let Some(cached) = self.gigya.lock().expect("gigya lock").clone() {
            return Ok(cached);
        }
        let answer: wire::StoreConfigEnvelope = self
            .gql(
                "GetStoreConfigForGigya",
                gql::STORE_CONFIG_FOR_GIGYA,
                serde_json::json!({}),
                Auth::None,
            )
            .await?;
        let config = answer.store_config.unwrap_or_default();
        let gigya = GigyaConfig {
            enabled: config.gigya_enable.unwrap_or(true),
            api_key: config.gigya_api_key.ok_or_else(|| {
                Error::Shape(format!(
                    "{} did not say which Gigya site it signs in against",
                    self.banner.name()
                ))
            })?,
            data_center: config.gigya_data_center.unwrap_or_else(|| "au1".into()),
            login_screen_set: config.gigya_login_screen_set,
            login_start_screen: config.gigya_login_start_screen,
        };
        *self.gigya.lock().expect("gigya lock") = Some(gigya.clone());
        Ok(gigya)
    }

    // ----------------------------------------------------------- the catalogue

    /// Products for a query. Needs no credentials of any kind.
    pub async fn listing(&self, query: &Query) -> Result<Listing> {
        let storefront = self.storefront().await?;
        let (Some(host), Some(key)) = (&storefront.klevu_url, &storefront.klevu_key) else {
            return Err(Error::Shape(format!(
                "{} did not publish a search index to query",
                self.banner.name()
            )));
        };
        let url = self.endpoints.klevu_search(host);
        let body = query.body(key).to_string();
        self.trace("POST", &url);
        let (_, text) = net_kit::http::text(
            "POST",
            &url,
            self.http
                .post(&url)
                .header(wreq::header::CONTENT_TYPE, "application/json")
                .header(wreq::header::ORIGIN, &self.endpoints.origin)
                .body(body)
                .send()
                .await,
        )
        .await?;
        search::listing(self.banner, &text)
            .map_err(|e| Error::decode("reading the search answer", e))
    }

    /// One product, in full.
    pub async fn product(&self, sku: &str) -> Result<ProductDetail> {
        let answer: wire::ProductsEnvelope = self
            .gql(
                "getProductDetailForProductPageBySku",
                gql::PRODUCT_DETAIL,
                serde_json::json!({ "sku": sku }),
                Auth::None,
            )
            .await?;
        let node = answer
            .products
            .unwrap_or_default()
            .items
            .into_iter()
            .next()
            .ok_or_else(|| Error::NoSuchProduct(sku.to_string()))?;

        // Price is a second call because it is customer-group dependent: a
        // signed-in member sees a different number from a guest, and the detail
        // query does not carry it.
        let prices: wire::ProductsEnvelope = self
            .gql(
                "getProductPricingBySku",
                gql::PRODUCT_PRICING,
                serde_json::json!({ "skus": [sku] }),
                if self.session().token.is_some() {
                    Auth::Required
                } else {
                    Auth::None
                },
            )
            .await?;
        let price = prices
            .products
            .unwrap_or_default()
            .items
            .into_iter()
            .next()
            .and_then(|p| p.price_range);

        // The specs the page prints, rather than the merchandising bag behind
        // it. Best effort: a product with no spec sheet is ordinary, and
        // losing it should cost a table rather than the command.
        let specs = self
            .gql::<wire::SpecsEnvelope>(
                "GetProductSpecs",
                gql::PRODUCT_SPECS,
                serde_json::json!({ "sku": sku }),
                Auth::None,
            )
            .await
            .ok()
            .and_then(|s| s.product_specs)
            .unwrap_or_default();

        let fulfilment = node.fulfilment();
        Ok(ProductDetail {
            sku: node.sku.clone().unwrap_or_else(|| sku.to_string()),
            name: node.name.clone().unwrap_or_default(),
            brand: node.brand.clone(),
            description: node.product_description.clone(),
            url_key: node.url_key.clone(),
            in_stock: node.in_stock(),
            price: price
                .as_ref()
                .and_then(|r| r.minimum_price.as_ref())
                .and_then(|p| p.final_price.as_ref())
                .and_then(wire::MoneyNode::money),
            was_price: price
                .as_ref()
                .and_then(|r| r.minimum_price.as_ref())
                .and_then(|p| p.regular_price.as_ref())
                .and_then(wire::MoneyNode::money),
            images: node
                .image
                .iter()
                .chain(node.media_gallery.iter())
                .filter_map(|i| i.url.clone())
                .collect(),
            attributes: specs.attributes(),
            categories: node
                .categories
                .iter()
                .filter_map(|c| c.name.clone())
                .collect(),
            fulfilment,
            variants: node
                .variants
                .iter()
                .filter_map(wire::VariantNode::variant)
                .collect(),
        })
    }

    /// The whole category tree, flat. Small enough to fetch in one go.
    pub async fn categories(&self) -> Result<Vec<Category>> {
        let answer: wire::CategoriesEnvelope = self
            .gql(
                "GetCategories",
                gql::CATEGORIES,
                serde_json::json!({}),
                Auth::None,
            )
            .await?;
        Ok(answer
            .categories
            .unwrap_or_default()
            .items
            .iter()
            .filter_map(wire::CategoryNode::category)
            .collect())
    }

    /// What a pasted URL points at.
    pub async fn resolve(&self, url: &str) -> Result<serde_json::Value> {
        let path = url
            .split_once("://")
            .map(|(_, rest)| rest.split_once('/').map(|(_, p)| p).unwrap_or(""))
            .map(|p| format!("/{p}"))
            .unwrap_or_else(|| url.to_string());
        let answer: serde_json::Value = self
            .gql(
                "ResolveURL",
                gql::RESOLVE_URL,
                serde_json::json!({ "url": path }),
                Auth::None,
            )
            .await?;
        Ok(answer)
    }

    // --------------------------------------------------------------- stores

    /// Every store, with addresses, hours and click-and-collect facts.
    ///
    /// Two calls because the storefront splits them: `getRegion` has the
    /// addresses and `getStoreLocator` has the collection flags, and neither
    /// has both.
    pub async fn stores(&self) -> Result<Vec<Store>> {
        let regions: wire::RegionsEnvelope = self
            .gql("GET_STORES", gql::STORES, serde_json::json!({}), Auth::None)
            .await?;
        let mut stores: Vec<Store> = regions
            .regions
            .iter()
            .flat_map(|r| {
                r.store_items
                    .iter()
                    .map(|s| s.store(r.region_name.as_deref()))
            })
            .collect();

        // Best effort: the addresses are the useful half, and losing the
        // collection flags should not cost the list.
        if let Ok(cnc) = self
            .gql::<wire::ClickAndCollectEnvelope>(
                "GetStoreClickAndCollect",
                gql::STORES_CLICK_AND_COLLECT,
                serde_json::json!({}),
                Auth::None,
            )
            .await
        {
            for store in &mut stores {
                if let Some(flags) = cnc.stores.iter().find(|c| c.store_id == store.id) {
                    store.click_and_collect = flags.is_click_and_collect;
                    store.same_day_cutoff = flags.same_day_delivery.clone();
                }
            }
        }
        Ok(stores)
    }

    /// Remember a store against the account. Needs a token.
    pub async fn set_store(&self, fulfilment_number: &str) -> Result<()> {
        let _: serde_json::Value = self
            .gql(
                "SetCustomerStoreLocator",
                gql::SET_STORE_LOCATOR,
                serde_json::json!({ "fulfilment_number": fulfilment_number }),
                Auth::Required,
            )
            .await?;
        Ok(())
    }

    // ---------------------------------------------------------------- stock

    /// Click-and-collect stock for one product at one store.
    ///
    /// Takes a SKU and its [`Fulfilment`] rather than a [`ProductDetail`]
    /// because a configurable product cannot answer for itself: only its
    /// variants carry a barcode, and the service identifies a product by
    /// barcode. [`ProductDetail::stockable`] is what turns a product into the
    /// one or many things that *can* be asked about.
    ///
    /// `store_id` is the store's `store_id`. Not its `fulfilment_number` --
    /// that one is for `setStoreLocator`, and mixing them up answers
    /// `NOT_FOUND_STORE` as a success.
    pub async fn stock(&self, sku: &str, fulfilment: &Fulfilment, store_id: i64) -> Result<Stock> {
        let line = stock::Line::new(sku, fulfilment, 1).ok_or_else(|| {
            Error::Shape(format!(
                "{sku} has no barcode, which the stock service needs to identify it"
            ))
        })?;
        self.availability(vec![sku.to_string()], vec![line], store_id)
            .await
    }

    /// Whether a whole basket can be collected from one store.
    ///
    /// One verdict, which is what the service answers: the worst line decides
    /// it. This is the question the website asks about a cart, and it is not
    /// the same as asking about each line and reading the answers -- for that,
    /// call [`Client::stock`] per item.
    ///
    /// Lines whose product has no barcode are dropped rather than failing the
    /// request, because one invalid line is rejected for the whole basket.
    pub async fn basket_stock(
        &self,
        items: &[(String, Fulfilment, u32)],
        store_id: i64,
    ) -> Result<Stock> {
        let mut skus = Vec::new();
        let mut lines = Vec::new();
        for (sku, fulfilment, quantity) in items {
            if let Some(line) = stock::Line::new(sku, fulfilment, *quantity) {
                skus.push(sku.clone());
                lines.push(line);
            }
        }
        if lines.is_empty() {
            return Err(Error::Shape(
                "nothing in this basket carries a barcode the stock service can identify".into(),
            ));
        }
        self.availability(skus, lines, store_id).await
    }

    async fn availability(
        &self,
        skus: Vec<String>,
        lines: Vec<stock::Line>,
        store_id: i64,
    ) -> Result<Stock> {
        let request = stock::Request {
            store_code: self.banner.store_code(),
            selected_store_id: store_id.to_string(),
            line_items: lines,
        };
        let url = self.endpoints.availability();
        self.trace("POST", &url);
        let (_, text) = net_kit::http::text(
            "POST",
            &url,
            self.http
                .post(&url)
                .header(wreq::header::CONTENT_TYPE, "application/json")
                .json(&request)
                .send()
                .await,
        )
        .await?;
        stock::stock(skus, store_id, &text)
            .map_err(|e| Error::decode("reading the stock answer", e))
    }

    // ----------------------------------------------------------------- cart

    /// The signed-in account's cart, creating one if there is none.
    pub async fn cart_id(&self) -> Result<String> {
        let answer: wire::CustomerCartEnvelope = self
            .gql(
                "createCustomerCartFromCart",
                gql::CUSTOMER_CART,
                serde_json::json!({}),
                Auth::Required,
            )
            .await?;
        answer
            .customer_cart
            .and_then(|c| c.id)
            .ok_or_else(|| Error::Shape("the storefront named no cart for this account".into()))
    }

    pub async fn cart(&self, cart_id: &str) -> Result<Cart> {
        let answer: wire::CartEnvelope = self
            .gql(
                "GetCartDetails",
                &with_fragment(gql::GET_CART),
                serde_json::json!({ "cartId": cart_id }),
                Auth::Required,
            )
            .await?;
        Ok(answer
            .cart
            .ok_or_else(|| Error::Shape("no such cart".into()))?
            .cart())
    }

    pub async fn cart_add(&self, cart_id: &str, sku: &str, quantity: f64) -> Result<Cart> {
        self.cart_mutation(
            "AddProductToCart",
            gql::ADD_TO_CART,
            serde_json::json!({
                "cartId": cart_id,
                "product": { "sku": sku, "quantity": quantity },
            }),
        )
        .await
    }

    pub async fn cart_update(&self, cart_id: &str, item: &str, quantity: f64) -> Result<Cart> {
        self.cart_mutation(
            "updateItemQuantity",
            gql::UPDATE_CART_ITEM,
            serde_json::json!({ "cartId": cart_id, "itemId": item, "quantity": quantity }),
        )
        .await
    }

    pub async fn cart_remove(&self, cart_id: &str, item: &str) -> Result<Cart> {
        self.cart_mutation(
            "removeItem",
            gql::REMOVE_CART_ITEM,
            serde_json::json!({ "cartId": cart_id, "itemId": item }),
        )
        .await
    }

    pub async fn cart_coupon(&self, cart_id: &str, code: &str) -> Result<Cart> {
        self.cart_mutation(
            "applyCouponsToCart",
            gql::APPLY_COUPON,
            serde_json::json!({ "cartId": cart_id, "codes": [code] }),
        )
        .await
    }

    async fn cart_mutation(
        &self,
        operation: &'static str,
        document: &str,
        variables: serde_json::Value,
    ) -> Result<Cart> {
        let answer: wire::CartMutationEnvelope = self
            .gql(
                operation,
                &with_fragment(document),
                variables,
                Auth::Required,
            )
            .await?;
        let result = answer
            .result
            .ok_or_else(|| Error::Shape(format!("{operation} answered with nothing")))?;
        // Magento reports a refused line here rather than in `errors`, so a
        // caller that only checked the envelope would call this a success.
        if let Some(first) = result.user_errors.first() {
            return Err(Error::Graphql {
                operation,
                message: first
                    .message
                    .clone()
                    .unwrap_or_else(|| "the storefront refused it without saying why".into()),
            });
        }
        Ok(result
            .cart
            .ok_or_else(|| Error::Shape(format!("{operation} returned no cart")))?
            .cart())
    }

    // ------------------------------------------------------------- wishlist

    pub async fn wishlist(&self) -> Result<Wishlist> {
        let answer: wire::CustomerEnvelope<wire::WishlistCustomer> = self
            .gql(
                "GetCustomerWishlist",
                gql::WISHLIST,
                serde_json::json!({ "currentPage": 1 }),
                Auth::Required,
            )
            .await?;
        answer
            .customer
            .unwrap_or_default()
            .wishlists
            .first()
            .map(wire::WishlistNode::wishlist)
            .ok_or_else(|| Error::Shape("this account has no wishlist".into()))
    }

    /// Add to the default list. `"0"` is how the storefront spells "the one
    /// list this account has" -- both fascias answer false to
    /// `getMultipleWishlistsEnabled`.
    pub async fn wishlist_add(&self, sku: &str, quantity: f64) -> Result<u64> {
        self.wishlist_mutation(
            "AddProductToWishlistFromGallery",
            gql::ADD_TO_WISHLIST,
            serde_json::json!({
                "wishlistId": "0",
                "itemOptions": { "sku": sku, "quantity": quantity },
            }),
        )
        .await
    }

    pub async fn wishlist_remove(&self, list: &str, item: &str) -> Result<u64> {
        self.wishlist_mutation(
            "RemoveProductsFromWishlist",
            gql::REMOVE_FROM_WISHLIST,
            serde_json::json!({ "wishlistId": list, "wishlistItemsId": [item] }),
        )
        .await
    }

    async fn wishlist_mutation(
        &self,
        operation: &'static str,
        document: &str,
        variables: serde_json::Value,
    ) -> Result<u64> {
        let answer: wire::WishlistMutationEnvelope = self
            .gql(operation, document, variables, Auth::Required)
            .await?;
        let result = answer
            .result
            .ok_or_else(|| Error::Shape(format!("{operation} answered with nothing")))?;
        if let Some(first) = result.user_errors.first() {
            return Err(Error::Graphql {
                operation,
                message: first
                    .message
                    .clone()
                    .unwrap_or_else(|| "the storefront refused it without saying why".into()),
            });
        }
        Ok(result.wishlist.map(|w| w.items_count).unwrap_or(0))
    }

    // --------------------------------------------------------------- account

    pub async fn customer(&self) -> Result<Customer> {
        let answer: wire::CustomerEnvelope<wire::CustomerNode> = self
            .gql(
                "GetCustomerAfterSignIn",
                gql::CUSTOMER,
                serde_json::json!({}),
                Auth::Required,
            )
            .await?;
        Ok(answer
            .customer
            .ok_or_else(|| Error::SessionExpired {
                banner: self.banner.name(),
            })?
            .customer())
    }

    /// Orders placed online.
    pub async fn orders(&self, page: u64, page_size: u64) -> Result<OrderPage> {
        let answer: wire::OrderListEnvelope = self
            .gql(
                "getHistoryOrderList",
                gql::ORDER_LIST,
                serde_json::json!({ "currentPage": page, "pageSize": page_size }),
                Auth::Required,
            )
            .await?;
        let list = answer.list.unwrap_or_default();
        let info = list.page_info.unwrap_or_default();
        Ok(OrderPage {
            orders: list
                .items
                .iter()
                .filter_map(wire::OrderNode::order)
                .collect(),
            page: info.current_page,
            pages: info.total_pages,
        })
    }

    pub async fn order(&self, number: &str) -> Result<OrderDetail> {
        let answer: wire::CustomerEnvelope<wire::OrdersCustomer> = self
            .gql(
                "getOrderDetailsFromOrderHistory",
                gql::ORDER_DETAIL,
                serde_json::json!({ "number": number }),
                Auth::Required,
            )
            .await?;
        answer
            .customer
            .unwrap_or_default()
            .orders
            .unwrap_or_default()
            .items
            .first()
            .and_then(wire::OrderDetailNode::detail)
            .ok_or_else(|| Error::Shape(format!("no order numbered {number}")))
    }

    /// Receipts from walking into a shop. A different history from [`Client::orders`].
    pub async fn receipts(
        &self,
        page: u64,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<ReceiptPage> {
        let answer: wire::CustomerEnvelope<wire::ReceiptsCustomer> = self
            .gql(
                "CustomerOrdersHistory",
                gql::IN_STORE_ORDERS,
                serde_json::json!({
                    "page": page,
                    "startDate": from,
                    "endDate": to,
                    "reference": serde_json::Value::Null,
                }),
                Auth::Required,
            )
            .await?;
        let history = answer
            .customer
            .unwrap_or_default()
            .history
            .unwrap_or_default();
        let receipts: Vec<Receipt> = history
            .items
            .iter()
            .filter_map(wire::ReceiptNode::receipt)
            .collect();
        // The window is the service's own choice when none was asked for, and
        // it only says so on the records themselves.
        let window = history.items.first();
        Ok(ReceiptPage {
            from: from
                .map(str::to_string)
                .or_else(|| window.and_then(|r| r.start_date.clone())),
            to: to
                .map(str::to_string)
                .or_else(|| window.and_then(|r| r.end_date.clone())),
            more_history: history.message_type.as_deref() == Some(wire::MORE_HISTORY),
            receipts,
        })
    }

    pub async fn loyalty(&self, force_refresh: bool) -> Result<Loyalty> {
        #[derive(serde::Deserialize)]
        struct Envelope {
            customer: Option<wire::LoyaltyCustomer>,
            #[serde(rename = "storeConfig")]
            store_config: Option<wire::StoreConfig>,
        }
        let answer: Envelope = self
            .gql(
                "GetLoyaltyDashboard",
                gql::LOYALTY,
                serde_json::json!({ "forceRefresh": force_refresh }),
                Auth::Required,
            )
            .await?;
        let description = answer
            .store_config
            .and_then(|c| c.loyalty_my_account_description);
        Ok(answer.customer.unwrap_or_default().loyalty(description))
    }

    pub async fn invoice_pdf(&self, invoice_id: &str) -> Result<String> {
        let answer: serde_json::Value = self
            .gql(
                "InvoicePdf",
                gql::INVOICE_PDF,
                serde_json::json!({ "invoiceId": invoice_id }),
                Auth::Required,
            )
            .await?;
        answer["customer"]["invoicePdf"]["pdf_url"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| {
                Error::Shape(
                    answer["customer"]["invoicePdf"]["message"]
                        .as_str()
                        .unwrap_or("the storefront produced no invoice URL")
                        .to_string(),
                )
            })
    }

    // ------------------------------------------------------------------ auth

    /// Mint a Magento token from the stored Gigya login.
    ///
    /// The refresh path, and the reason a browser is a once-only cost. Needs no
    /// captcha: `accounts.getAccountInfo` carries no risk assessment, and
    /// `loginGigya` verifies a signature rather than a password.
    pub async fn renew(&self) -> Result<()> {
        if self.session().token_fresh() {
            return Ok(());
        }
        let session = self.session();
        let Some(login_token) = session.login_token.clone() else {
            return Err(if session.token.is_some() {
                Error::SessionExpired {
                    banner: self.banner.name(),
                }
            } else {
                Error::NotSignedIn {
                    banner: self.banner.name(),
                }
            });
        };

        let gigya = self.gigya_config().await?;
        let assertion = auth::assert(
            &self.http,
            &self.endpoints,
            self.banner,
            &gigya.api_key,
            &login_token,
            session.email.as_deref(),
        )
        .await?;

        let token = self.exchange(&assertion).await?;
        let mut next = session;
        next.set_token(token);
        next.uid = Some(assertion.uid);
        next.email = Some(assertion.email);
        self.file(next);
        Ok(())
    }

    /// Trade an identity assertion for a customer token.
    pub async fn exchange(&self, assertion: &auth::Assertion) -> Result<String> {
        let answer: wire::TokenEnvelope = self
            .gql(
                "LoginGigya",
                gql::LOGIN_GIGYA,
                serde_json::json!({
                    "email": assertion.email,
                    "gigyaUid": assertion.uid,
                    "gigyaUidSignature": assertion.uid_signature,
                    "signatureTimestamp": assertion.signature_timestamp,
                }),
                Auth::None,
            )
            .await?;
        answer
            .result
            .and_then(|t| t.token)
            .ok_or_else(|| Error::Shape("the storefront minted no customer token".into()))
    }

    /// Take a completed sign-in and file it.
    pub fn adopt(&self, login: auth::Login, token: String) {
        let mut session = self.session();
        session.login_token = Some(login.login_token);
        session.uid = Some(login.assertion.uid);
        session.email = Some(login.assertion.email);
        session.set_token(token);
        self.file(session);
    }

    fn file(&self, session: Session) {
        *self.session.lock().expect("session lock") = session.clone();
        if let Some(store) = &self.store {
            // Best effort: failing to cache a token must not fail the command
            // the token was minted for.
            let _ = crate::session::StoredSession::save(&store.secrets, self.banner, &session);
        }
    }

    // ------------------------------------------------------------- transport

    /// One GraphQL call.
    ///
    /// **POST, always.** The storefront's own frontend sends queries as GET so
    /// its CDN can key on them, but the endpoint answers `cache-control:
    /// no-store` regardless -- there is nothing to cache, and a GET puts a
    /// three-kilobyte document in a URL for no gain.
    async fn gql<T: DeserializeOwned>(
        &self,
        operation: &'static str,
        document: &str,
        variables: serde_json::Value,
        auth: Auth,
    ) -> Result<T> {
        if auth == Auth::Required {
            // Boxed to break a cycle that is real in the types and not at run
            // time: `renew` asks `gigya_config`, which is an unauthenticated
            // call back through here. `Auth::None` is what terminates it.
            Box::pin(self.renew()).await?;
        }
        let url = self.endpoints.graphql();
        let payload = serde_json::json!({
            "operationName": operation,
            "variables": variables,
            "query": document,
        });
        self.trace("POST", &format!("{url} ({operation})"));

        let mut req = self
            .http
            .post(&url)
            .header(wreq::header::CONTENT_TYPE, "application/json")
            // The one header that decides which catalogue answers. Omitting it
            // is not an error, it is the wrong shop.
            .header("store", self.banner.store_code())
            .header(wreq::header::ORIGIN, &self.endpoints.origin)
            .header(wreq::header::REFERER, format!("{}/", self.endpoints.origin));
        if auth == Auth::Required {
            if let Some(bearer) = self.session().bearer() {
                req = req.header(wreq::header::AUTHORIZATION, bearer);
            }
        }

        let res = req
            .body(payload.to_string())
            .send()
            .await
            .map_err(|source| net_kit::HttpError::Transport {
                method: "POST",
                url: url.clone(),
                source,
            })?;
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.text().await.unwrap_or_default();

        if status.as_u16() == 429 {
            return Err(Error::RateLimited {
                banner: self.banner.name(),
                retry_after: headers
                    .get(wreq::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok()),
            });
        }

        let envelope: wire::GqlEnvelope<T> = serde_json::from_str(&body).map_err(|e| {
            if !status.is_success() {
                return Error::Http(net_kit::HttpError::Status {
                    method: "POST",
                    url: url.clone(),
                    status: status.as_u16(),
                    detail: net_kit::error::truncate(&body, 300),
                    body: body.clone(),
                });
            }
            Error::decode(format!("reading the answer to {operation}"), e)
        })?;

        if let Some(error) = envelope.errors.first() {
            let message = error.message.clone().unwrap_or_default();
            return Err(match error.category() {
                Some("graphql-authorization") => Error::SessionExpired {
                    banner: self.banner.name(),
                },
                _ if message.contains("current customer isn't authorized") => {
                    Error::SessionExpired {
                        banner: self.banner.name(),
                    }
                }
                _ => Error::Graphql { operation, message },
            });
        }

        envelope.data.ok_or_else(|| Error::Graphql {
            operation,
            message: "the storefront answered with neither data nor an error".into(),
        })
    }

    /// Narrate a request on stderr. Never a credential: no headers, no bodies.
    fn trace(&self, method: &str, url: &str) {
        if self.debug {
            eprintln!("bgnz: {method} {url}");
        }
    }
}

/// Cart documents all use one fragment; joining it on here keeps the fragment
/// written once rather than pasted into five documents that then drift.
fn with_fragment(document: &str) -> String {
    format!("{document}{}", gql::CART_FRAGMENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cart_document_carries_the_fragment_it_spreads() {
        let document = with_fragment(gql::GET_CART);
        assert!(document.contains("...CartDetail"), "spreads it");
        assert!(
            document.contains("fragment CartDetail on Cart"),
            "defines it"
        );
    }
}
