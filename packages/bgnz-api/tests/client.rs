//! Integration tests against a mock storefront.
//!
//! Aimed at the three things that fail *silently* against the real sites and so
//! cannot be caught by reading an error message: the `store` header deciding
//! which catalogue answers, the Klevu key deciding which index answers, and the
//! renewal path quietly not happening.

use bgnz_api::{Banner, Client, Endpoints, Query, Session};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn client(server: &MockServer, banner: Banner, session: Session) -> Client {
    let http = net_kit::http::build(bgnz_api::client_spec()).expect("a client builds");
    let endpoints = Endpoints::defaults(banner)
        .with_origin(server.uri())
        .with_gigya(server.uri());
    Client::new(http, endpoints, banner, session)
}

/// A GraphQL answer, as the storefront sends them.
fn data(value: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": value }))
}

#[tokio::test]
async fn every_graphql_call_names_the_fascia_it_wants() {
    // The header Magento does not require and answers wrongly without: a
    // missing one serves Briscoes, so a Rebel Sport command would come back
    // with homeware and no error at all.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("store", "rebelsport"))
        .respond_with(data(serde_json::json!({
            "storeConfig": {
                "store_code": "rebelsport",
                "klevu_search_url": "aucs34.ksearchnet.com",
                "klevu_search_js_api_key": "klevu-173190029190817559",
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let storefront = client(&server, Banner::RebelSport, Session::default())
        .storefront()
        .await
        .expect("storeConfig answers");
    assert_eq!(storefront.store_code, "rebelsport");
}

#[tokio::test]
async fn a_search_goes_to_the_index_the_storefront_named() {
    // Both the host and the key are per-fascia and published at run time. A
    // wrong key is not refused -- Klevu answers it with nothing.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(data(serde_json::json!({
            "storeConfig": {
                "store_code": "briscoes",
                "klevu_search_url": server.uri(),
                "klevu_search_js_api_key": "klevu-173190000117617559",
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/cs/v2/search"))
        .and(body_string_contains("klevu-173190000117617559"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "meta": { "responseCode": 200 },
            "queryResults": [{
                "id": "productList",
                "meta": { "totalResultsFound": 173, "offset": 0 },
                "records": [{
                    "sku": "1103832",
                    "name": "Prestige Lotus Filter Jug 2.4 Litre HS528",
                    "price": "79.99",
                    "salePrice": "59.99",
                    "inStock": "yes",
                }],
            }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let listing = client(&server, Banner::Briscoes, Session::default())
        .listing(&Query::search("jug"))
        .await
        .expect("the index answers");
    assert_eq!(listing.total, 173);
    assert_eq!(listing.products[0].sku, "1103832");
    assert_eq!(listing.products[0].was_price, Some(79.99));
}

#[tokio::test]
async fn an_expired_token_is_renewed_from_the_stored_gigya_login_without_a_captcha() {
    // The whole reason a browser is a once-only cost. Three calls in order:
    // ask Gigya for a fresh signature, trade it for a customer token, then make
    // the call that was asked for.
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/accounts.getAccountInfo"))
        .and(body_string_contains("login_token=st2.s.StoredLogin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "errorCode": 0,
            "UID": "101669781",
            "UIDSignature": "XqQOdSsl3OVVQnBUOfP59HSysXk=",
            "signatureTimestamp": "1789097018",
            "profile": { "email": "shopper@example.invalid" },
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("GetStoreConfigForGigya"))
        .respond_with(data(serde_json::json!({
            "storeConfig": {
                "store_code": "briscoes",
                "gigya_enable": true,
                "gigya_api_key": "4_R5DCbpg6mrfoot48AR2Wcg",
                "gigya_data_center": "au1",
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("LoginGigya"))
        .respond_with(data(
            serde_json::json!({ "loginGigya": { "token": "minted.customer.token" } }),
        ))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("GetCustomerAfterSignIn"))
        .and(header("authorization", "Bearer minted.customer.token"))
        .respond_with(data(serde_json::json!({
            "customer": {
                "email": "shopper@example.invalid",
                "firstname": "Sam",
                "customer_group": { "group_code": "ZW-ZW" },
                "custom_attributes": [
                    { "code": "customer_type_code_briscoes", "value": "ZW" }
                ],
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let session = Session {
        login_token: Some("st2.s.StoredLogin".into()),
        ..Default::default()
    };
    let client = client(&server, Banner::Briscoes, session);
    let customer = client.customer().await.expect("the account answers");

    assert_eq!(customer.email.as_deref(), Some("shopper@example.invalid"));
    assert_eq!(customer.customer_type.as_deref(), Some("ZW"));
    // The renewed token is held for the rest of the run, not thrown away.
    assert_eq!(
        client.session().token.as_deref(),
        Some("minted.customer.token")
    );
}

#[tokio::test]
async fn a_call_with_no_stored_login_says_so_rather_than_asking_gigya() {
    // Nothing to refresh from is a different failure from a refused refresh,
    // and spending a request to discover it would be wasted.
    let server = MockServer::start().await;
    let client = client(&server, Banner::RebelSport, Session::default());
    let error = client.customer().await.expect_err("there is no account");
    assert!(
        matches!(error, bgnz_api::Error::NotSignedIn { .. }),
        "{error:?}"
    );
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        0
    );
}

#[tokio::test]
async fn a_missing_captcha_is_reported_as_needing_a_browser() {
    // What the live site actually answers a password-only sign-in with. It
    // must not read as a bad password: retyping one never fixes it.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/accounts.login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "errorCode": 400006,
            "errorMessage": "Invalid parameter value",
            "errorDetails": "Invalid CaptchaType\nInvalid CaptchaToken",
            "statusCode": 400,
        })))
        .mount(&server)
        .await;

    let http = net_kit::http::build(bgnz_api::client_spec()).expect("a client builds");
    let endpoints = Endpoints::defaults(Banner::Briscoes).with_gigya(server.uri());
    let error = bgnz_api::auth::login(
        &http,
        &endpoints,
        Banner::Briscoes,
        "4_R5DCbpg6mrfoot48AR2Wcg",
        "shopper@example.invalid",
        "not-a-real-password",
        "",
    )
    .await
    .expect_err("a captcha-less sign-in is refused");

    assert!(error.needs_browser(), "{error:?}");
    assert!(!bgnz_api::auth::is_bad_credentials(&error), "{error:?}");
}

#[tokio::test]
async fn a_lapsed_gigya_login_is_not_retried_as_an_expired_token() {
    // `is_lapsed` gates an automatic renewal, so this answering true would
    // loop: renew, fail, renew.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("GetStoreConfigForGigya"))
        .respond_with(data(serde_json::json!({
            "storeConfig": { "gigya_api_key": "4_R5DCbpg6mrfoot48AR2Wcg" }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/accounts.getAccountInfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "errorCode": 403005,
            "errorMessage": "Unauthorized user",
        })))
        .mount(&server)
        .await;

    let session = Session {
        login_token: Some("st2.s.Expired".into()),
        ..Default::default()
    };
    let error = client(&server, Banner::Briscoes, session)
        .customer()
        .await
        .expect_err("a dead login cannot renew");
    assert!(error.needs_browser(), "{error:?}");
    assert!(!error.is_lapsed(), "{error:?}");
}

#[tokio::test]
async fn the_storefronts_authorization_error_reads_as_an_expired_session() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "errors": [{
                "message": "The current customer isn't authorized.",
                "extensions": { "category": "graphql-authorization" },
            }],
            "data": { "customer": null },
        })))
        .mount(&server)
        .await;

    // A token that parses as fresh, so renewal is not attempted and the
    // storefront's own verdict is what surfaces.
    let mut session = Session {
        login_token: Some("st2.s.Live".into()),
        ..Default::default()
    };
    session.set_token("opaque-token".into());
    let error = client(&server, Banner::Briscoes, session)
        .customer()
        .await
        .expect_err("the storefront refused it");
    assert!(error.is_lapsed(), "{error:?}");
}

#[tokio::test]
async fn stock_is_asked_for_by_store_id_and_names_the_fascia() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("getProductDetailForProductPageBySku"))
        .respond_with(data(serde_json::json!({
            "products": { "total_count": 1, "items": [{
                "sku": "1103832",
                "name": "Prestige Lotus Filter Jug 2.4 Litre HS528",
                "stock_status": "IN_STOCK",
                "barcode": "9414533081169",
                "sapcategory": 141,
                "subcategory": "14150",
                "shipping_band": "Standard",
                "isdropship": 0,
                "saleavailability_label": null,
            }]}
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("getProductPricingBySku"))
        .respond_with(data(serde_json::json!({ "products": { "items": [] } })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/web/cnc-commerce/availability"))
        .respond_with(|req: &Request| {
            let body: serde_json::Value = req.body_json().expect("a JSON body");
            // The trap this test exists for: the availability service takes a
            // store_id, answers NOT_FOUND_STORE for a fulfilment number, and
            // calls that a success.
            assert_eq!(body["selectedStoreId"], "291");
            assert_eq!(body["storeCode"], "briscoes");
            assert_eq!(body["lineItems"][0]["barcode"], "9414533081169");
            assert_eq!(body["lineItems"][0]["category"], "141");
            assert!(body["lineItems"][0]["saleAvailable"].is_string());
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "pickupStatus": "IN_STOCK_AFTER_CUTOFF",
                    "stockLevel": "in-stock",
                    "message": { "key": "next_day_timeframe", "value": "Pick up tomorrow" },
                },
                "messageCode": "SUCCESS",
                "success": true,
            }))
        })
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server, Banner::Briscoes, Session::default());
    let product = client
        .product("1103832")
        .await
        .expect("the product answers");
    let (sku, fulfilment) = *product
        .stockable()
        .first()
        .expect("a simple product answers for itself");
    let stock = client
        .stock(sku, fulfilment, 291)
        .await
        .expect("the stock service answers");
    assert_eq!(
        stock.pickup_status.as_deref(),
        Some("IN_STOCK_AFTER_CUTOFF")
    );
    assert_eq!(stock.message.as_deref(), Some("Pick up tomorrow"));
}

#[tokio::test]
async fn a_configurable_product_is_asked_about_by_variant() {
    // Rebel Sport is mostly configurables, and a configurable carries
    // `barcode: null` -- the barcodes are on the variants, and the barcode is
    // what the stock service identifies a product by. Without this the whole
    // fascia would be unanswerable at a store.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("getProductDetailForProductPageBySku"))
        .respond_with(data(serde_json::json!({
            "products": { "total_count": 1, "items": [{
                "sku": "8237741",
                "name": "Asics Unisex Jetray Pro Football Boots",
                "stock_status": "IN_STOCK",
                "barcode": null,
                "sapcategory": "884",
                "subcategory": "88405",
                "variants": [
                    {
                        "attributes": [
                            { "code": "colourcode", "label": "White/Red" },
                            { "code": "sizecode", "label": "US10" }
                        ],
                        "product": {
                            "sku": "8237741004",
                            "barcode": "4571633959509",
                            "stock_status": "IN_STOCK",
                            "shipping_band": "Standard",
                            "isdropship": 0,
                        }
                    },
                    {
                        "attributes": [{ "code": "sizecode", "label": "US11" }],
                        "product": { "sku": "8237741009", "barcode": null }
                    }
                ],
            }]}
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("getProductPricingBySku"))
        .respond_with(data(serde_json::json!({ "products": { "items": [] } })))
        .mount(&server)
        .await;

    let product = client(&server, Banner::RebelSport, Session::default())
        .product("8237741")
        .await
        .expect("the product answers");

    assert_eq!(product.variants.len(), 2);
    assert_eq!(product.variants[0].label(), "White/Red / US10");

    // The parent cannot be asked about, and the variant without a barcode is
    // left out rather than being sent and failing the request.
    let stockable = product.stockable();
    assert_eq!(stockable.len(), 1, "{stockable:?}");
    assert_eq!(stockable[0].0, "8237741004");
    assert_eq!(stockable[0].1.barcode.as_deref(), Some("4571633959509"));
}

#[tokio::test]
async fn a_basket_is_one_question_and_skips_lines_it_cannot_identify() {
    // The service answers per basket, not per line, and one invalid line is
    // rejected for the whole request -- so a line with no barcode has to be
    // dropped before sending rather than after.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/web/cnc-commerce/availability"))
        .respond_with(|req: &Request| {
            let body: serde_json::Value = req.body_json().expect("a JSON body");
            let lines = body["lineItems"].as_array().expect("line items");
            assert_eq!(lines.len(), 1, "the barcodeless line is not sent");
            assert_eq!(lines[0]["quantity"], 2);
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "pickupStatus": "OOS", "stockLevel": "out-stock" },
                "messageCode": "SUCCESS",
                "success": true,
            }))
        })
        .expect(1)
        .mount(&server)
        .await;

    let with_barcode = bgnz_api::Fulfilment {
        barcode: Some("9414533081169".into()),
        category: Some("141".into()),
        subcategory: Some("14150".into()),
        ..Default::default()
    };
    let without = bgnz_api::Fulfilment::default();

    let stock = client(&server, Banner::Briscoes, Session::default())
        .basket_stock(
            &[
                ("1103832".into(), with_barcode, 2),
                ("1132853".into(), without, 1),
            ],
            291,
        )
        .await
        .expect("the service answers");
    assert_eq!(stock.skus, vec!["1103832".to_string()]);
    assert_eq!(stock.pickup_status.as_deref(), Some("OOS"));
}
