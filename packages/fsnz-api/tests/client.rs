//! The edge API against a mock server.
//!
//! Library-level: no binary is built and nothing touches the network. Endpoints
//! are pointed at the mock by *assigning a field*, which is the whole reason
//! this crate takes values rather than reading the environment.

use fsnz_api::{Banner, Client, Endpoints};
use net_kit::{ClientSpec, Fault};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn http() -> net_kit::wreq::Client {
    net_kit::http::build(ClientSpec::new(
        fsnz_api::EMULATION,
        net_kit::wreq::redirect::Policy::none(),
    ))
    .expect("building a client")
}

fn client(server: &MockServer) -> Client {
    let endpoints = Endpoints::defaults(Banner::NewWorld)
        .with_api(server.uri())
        .with_origin(server.uri());
    Client::new(http(), Banner::NewWorld, endpoints, "a-token")
}

fn product(id: &str, name: &str, cents: i64) -> serde_json::Value {
    json!({
        "productID": id,
        "productId": id,
        "name": name,
        "brand": "Anchor",
        "displayName": "2L",
        "availability": ["ONLINE"],
        "singlePrice": {
            "price": cents,
            "comparativePrice": { "pricePerUnit": 225, "measureDescription": "1L" }
        },
        "categoryTrees": [{ "level0": "Fridge, Deli & Eggs" }]
    })
}

#[tokio::test]
async fn stores_are_read_from_a_bare_array() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": "4147", "name": "New World Thorndon", "region": "Wellington" }
        ])))
        .mount(&server)
        .await;

    let stores = client(&server).stores().await.unwrap();
    assert_eq!(stores.len(), 1);
    assert_eq!(stores[0].id, "4147");
    assert_eq!(stores[0].banner, Banner::NewWorld);
}

#[tokio::test]
async fn stores_are_also_read_from_a_wrapped_object() {
    // Both shapes have been seen in the wild; neither is documented.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "stores": [{ "id": "1", "name": "PAK'nSAVE Petone" }]
        })))
        .mount(&server)
        .await;

    let stores = client(&server).stores().await.unwrap();
    assert_eq!(stores.len(), 1);
    assert_eq!(stores[0].name, "PAK'nSAVE Petone");
}

#[tokio::test]
async fn a_store_with_no_id_is_skipped_rather_than_failing_the_command() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "name": "Nameless, idless" },
            { "id": "4147", "name": "New World Thorndon" }
        ])))
        .mount(&server)
        .await;

    let stores = client(&server).stores().await.unwrap();
    assert_eq!(stores.len(), 1, "the usable one still comes through");
}

#[tokio::test]
async fn a_store_missing_a_name_gets_a_placeholder_not_an_empty_cell() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "id": "4147" }])))
        .mount(&server)
        .await;

    assert_eq!(client(&server).stores().await.unwrap()[0].name, "(unnamed)");
}

#[tokio::test]
async fn search_maps_a_product_including_its_image_and_url() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "products": [product("5010819-EA-000", "Blue Milk", 450)],
            "totalHits": 1,
            "totalPages": 1
        })))
        .mount(&server)
        .await;

    let result = client(&server)
        .collect("4147", "milk", "stores:4147", 20, "NI_POPULARITY_ASC")
        .await
        .unwrap();
    assert_eq!(result.total_available, 1);
    let p = &result.products[0];
    assert_eq!(p.price_cents, Some(450));
    assert_eq!(p.unit_measure.as_deref(), Some("1L"));
    assert_eq!(p.in_stock, Some(true));
    assert_eq!(p.department.as_deref(), Some("Fridge, Deli & Eggs"));
    // The leading number of the sku is the image CDN's key.
    assert!(p.image.as_deref().unwrap().contains("5010819.png"), "{p:?}");
    assert!(p.url.ends_with("/shop/product/5010819-EA-000"), "{p:?}");
}

#[tokio::test]
async fn a_response_that_drops_every_optional_field_still_yields_a_product() {
    // The governing rule: a renamed field narrows what can be shown, it does
    // not fail the command.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "products": [{ "productId": "X-EA-000" }]
        })))
        .mount(&server)
        .await;

    let result = client(&server)
        .collect("4147", "milk", "", 20, "NI_POPULARITY_ASC")
        .await
        .unwrap();
    let p = &result.products[0];
    assert_eq!(p.sku, "X-EA-000");
    assert_eq!(p.price_cents, None);
    assert_eq!(p.in_stock, None, "unknown, not out of stock");
}

#[tokio::test]
async fn search_pages_until_it_has_what_was_asked_for() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .and(body_partial_json(json!({ "page": 0 })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "products": [product("A-EA-000", "First", 100)],
            "totalHits": 2, "totalPages": 2
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .and(body_partial_json(json!({ "page": 1 })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "products": [product("B-EA-000", "Second", 200)],
            "totalHits": 2, "totalPages": 2
        })))
        .mount(&server)
        .await;

    let result = client(&server)
        .collect("4147", "milk", "", 2, "NI_POPULARITY_ASC")
        .await
        .unwrap();
    assert_eq!(result.products.len(), 2, "both pages were fetched");
    assert_eq!(result.products[1].name, "Second");
}

#[tokio::test]
async fn search_stops_at_the_limit_rather_than_draining_the_results() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "products": [
                product("A-EA-000", "First", 100),
                product("B-EA-000", "Second", 200),
                product("C-EA-000", "Third", 300)
            ],
            "totalHits": 300, "totalPages": 100
        })))
        .mount(&server)
        .await;

    let result = client(&server)
        .collect("4147", "milk", "", 2, "NI_POPULARITY_ASC")
        .await
        .unwrap();
    assert_eq!(result.products.len(), 2);
    assert_eq!(result.total_available, 300, "but it reports what exists");
}

#[tokio::test]
async fn the_department_tree_comes_back_nested() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/store/4147/categories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "name": "Fridge, Deli & Eggs",
                "children": [
                    { "name": "Milk", "children": [{ "name": "Blue Milk", "children": [] }] },
                    { "name": "Eggs", "children": [] }
                ]
            },
            // Promotional nodes sit alongside real departments; they are
            // reported as they arrive rather than guessed at.
            { "name": "Bonus Sticker Products", "children": [] }
        ])))
        .mount(&server)
        .await;

    let tree = client(&server).categories("4147").await.unwrap();
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].children.len(), 2);
    assert_eq!(tree[0].children[0].children[0].name, "Blue Milk");
    assert_eq!(tree[1].name, "Bonus Sticker Products");
}

#[tokio::test]
async fn a_401_is_a_typed_auth_failure_not_a_message_to_grep() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/store"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&server)
        .await;

    let err = client(&server).stores().await.unwrap_err();
    assert_eq!(err.auth(), Some(net_kit::AuthFault::Rejected));
}

#[tokio::test]
async fn an_upstream_body_survives_for_the_caller_that_needs_it() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/store"))
        .respond_with(ResponseTemplate::new(400).set_body_string("Store is not defined"))
        .mount(&server)
        .await;

    let err = client(&server).stores().await.unwrap_err();
    assert!(err.body().contains("Store is not defined"), "{err}");
    assert_eq!(err.auth(), None, "a 400 is not an auth failure");
}

#[tokio::test]
async fn a_guest_token_is_minted_from_the_storefront_cookie() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("set-cookie", "fs-user-token=abc.def.ghi; Path=/; HttpOnly")
                .append_header("set-cookie", "_dyid=tracking; Path=/")
                .set_body_string("<html></html>"),
        )
        .mount(&server)
        .await;

    let endpoints = Endpoints::defaults(Banner::NewWorld).with_origin(server.uri());
    let token = fsnz_api::token::mint_guest(&http(), Banner::NewWorld, &endpoints)
        .await
        .unwrap();
    assert_eq!(token, "abc.def.ghi");
}

#[tokio::test]
async fn a_storefront_that_sets_no_token_says_which_cookie_was_missing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html></html>"))
        .mount(&server)
        .await;

    let endpoints = Endpoints::defaults(Banner::NewWorld).with_origin(server.uri());
    let err = fsnz_api::token::mint_guest(&http(), Banner::NewWorld, &endpoints)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("fs-user-token"), "{err}");
}
