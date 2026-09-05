//! The client against a mock server.
//!
//! Every gateway operation posts to one path, so mocks are routed on the
//! `operationName` in the body -- the only thing distinguishing them. The
//! catalogue is routed on its path instead, since Constructor puts the term in
//! the URL.
//!
//! What is worth testing here rather than in a unit test is the wiring between
//! surfaces: that the catalogue needs no credentials while the gateway does,
//! that a challenge is told apart from a refusal, and that a cart change reads
//! its version before it writes.

use kmart_api::{auth, Client, Country, Endpoints, Session};
use net_kit::{ClientSpec, Fault};
use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn http() -> net_kit::wreq::Client {
    net_kit::http::build(ClientSpec::new(
        kmart_api::EMULATION,
        net_kit::wreq::redirect::Policy::none(),
    ))
    .expect("building a client")
}

/// An export carrying admission for both countries, as a browser would write.
fn cookies_txt() -> String {
    "\
# Netscape HTTP Cookie File
.kmart.co.nz\tTRUE\t/\tTRUE\t9999999999\t_abck\tnz-admission
.kmart.co.nz\tTRUE\t/\tTRUE\t9999999999\tbm_sz\tnz-bm
.kmart.com.au\tTRUE\t/\tTRUE\t9999999999\t_abck\tau-admission
"
    .into()
}

fn client_with(server: &MockServer, session: Session, country: Country) -> Client {
    Client::new(
        http(),
        Endpoints::all(server.uri()).with_search_key("key_test"),
        country,
        session,
        "test-visitor",
    )
}

/// The usual case: admitted, not signed in.
fn client(server: &MockServer) -> Client {
    client_with(
        server,
        auth::session_from_netscape(&cookies_txt()),
        Country::Nz,
    )
}

async fn mount_gql(server: &MockServer, operation: &str, body: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/gateway/graphql"))
        .and(body_string_contains(operation))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

fn search_body(results: serde_json::Value, total: u64, exact: u64) -> serde_json::Value {
    json!({
        "response": {
            "results": results,
            "total_num_results": total,
            "result_sources": {"token_match": {"count": exact}},
            "facets": [],
            "sort_options": [],
        }
    })
}

fn item(keycode: &str, name: &str, dollars: f64) -> serde_json::Value {
    json!({
        "value": name,
        "data": {
            "id": format!("P_{keycode}"),
            "variation_id": keycode,
            "url": format!("/product/{keycode}/"),
            "price": dollars,
            "prices": [{"type": "list", "amount": format!("{dollars:.2}"), "currency": "NZD"}],
            "Brand": "Anko",
            "Seller": ["Kmart"],
            "nationalInventory": false,
        },
        "variations": [],
    })
}

#[tokio::test]
async fn the_catalogue_needs_no_credentials_at_all() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/search/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_body(
            json!([item("43165537", "Milk Frother", 39.0)]),
            1,
            1,
        )))
        .mount(&server)
        .await;

    // No cookies, no token -- the state a fresh install is in.
    let client = client_with(&server, Session::new(), Country::Nz);
    let listing = client.search("milk frother", 1, 10).await.unwrap();
    assert_eq!(listing.products.len(), 1);
    assert_eq!(listing.products[0].keycode, "43165537");
    assert_eq!(listing.products[0].price.unwrap().cents, 3900);
}

#[tokio::test]
async fn a_product_lookup_refuses_a_near_miss() {
    // A retired keycode matches nothing exactly, and the index answers with
    // something similar. Returning that as the product asked for would be
    // worse than saying no.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/search/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_body(
            json!([item("99999999", "Something Else", 5.0)]),
            20,
            0,
        )))
        .mount(&server)
        .await;

    let err = client(&server).product("43165537").await.unwrap_err();
    assert!(
        matches!(err, kmart_api::Error::NoSuchProduct(ref k) if k == "43165537"),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_gateway_call_without_admission_says_so_before_spending_a_request() {
    let server = MockServer::start().await;
    // Nothing mounted: reaching the server at all would be the bug.
    let client = client_with(&server, Session::new(), Country::Nz);
    let err = client.postcodes("1010").await.unwrap_err();
    assert!(err.is_challenge(), "{err:?}");
    assert!(!err.is_lapsed(), "signing in would not help");
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn admission_for_one_country_is_not_admission_for_the_other() {
    let server = MockServer::start().await;
    mount_gql(
        &server,
        "getPostcodeSuggestions",
        json!({"data": {"postcodeQuery": [{"postcode": "1010", "suburb": "AUCKLAND CENTRAL", "state": "NI"}]}}),
    )
    .await;

    let only_nz = Session::new().with_cookies(
        Country::Nz,
        auth::from_netscape(&cookies_txt(), Country::Nz),
    );
    assert!(client_with(&server, only_nz.clone(), Country::Nz)
        .postcodes("1010")
        .await
        .is_ok());
    let err = client_with(&server, only_nz, Country::Au)
        .postcodes("3000")
        .await
        .unwrap_err();
    assert!(err.is_challenge(), "{err:?}");
}

#[tokio::test]
async fn a_bot_challenge_is_not_reported_as_rate_limiting() {
    let server = MockServer::start().await;
    // Akamai's actual answer: a 429 whose body is a challenge marker.
    Mock::given(method("POST"))
        .and(path("/gateway/graphql"))
        .respond_with(
            ResponseTemplate::new(429).set_body_string(r#"{"cpr_chlge":"true","t":"230881678"}"#),
        )
        .mount(&server)
        .await;

    let err = client(&server).postcodes("1010").await.unwrap_err();
    assert!(err.is_challenge(), "{err:?}");
    assert!(
        !matches!(err, kmart_api::Error::RateLimited { .. }),
        "waiting and retrying would never clear this"
    );
}

#[tokio::test]
async fn stock_reads_every_channel_independently() {
    let server = MockServer::start().await;
    mount_gql(
        &server,
        "getProductAvailability",
        json!({"data": {"getProductAvailability": {
            "postcode": "1010",
            "country": "NZ",
            "region": "METRO",
            "availability": {
                "HOME_DELIVERY": [{"keycode": "43165537", "poolName": "POOL", "stock": {"available": 0}}],
                "CLICK_AND_COLLECT": [{
                    "keycode": "43165537",
                    "stock": {"totalAvailable": 93},
                    "locations": [
                        {"fulfilment": {"isBuddyLocation": false, "locationId": "8229", "stock": {"available": 26}},
                         "location": {"locationId": "8229"}, "distanceInKm": 16.6},
                        {"fulfilment": {"isBuddyLocation": true, "locationId": "8212", "stock": {"available": 4}},
                         "location": {"locationId": "8212"}, "distanceInKm": 16.73}
                    ]
                }],
                "IN_STORE": null,
                "EXPRESS_DELIVERY": []
            }
        }}}),
    )
    .await;

    let a = client(&server)
        .availability("43165537", "1010", false)
        .await
        .unwrap();
    // Sold out for delivery and on a shelf at the same time: the state that
    // makes this per channel rather than a boolean.
    assert_eq!(a.home_delivery, Some(0));
    assert_eq!(a.click_and_collect, Some(93));
    assert!(a.any());
    assert_eq!(a.stores.len(), 2);
    assert_eq!(a.stores[0].distance_km, Some(16.6));
    assert!(a.stores[1].buddy, "a buddy store fills for another one");
    assert!(a.in_store.is_empty(), "null and [] both read as nothing");
}

#[tokio::test]
async fn the_store_finder_probes_availability_then_names_what_it_found() {
    let server = MockServer::start().await;
    mount_gql(
        &server,
        "getProductAvailability",
        json!({"data": {"getProductAvailability": {
            "postcode": "1010",
            "availability": {"CLICK_AND_COLLECT": [{
                "keycode": "9999",
                "stock": {"totalAvailable": 0},
                "locations": [{
                    "fulfilment": {"isBuddyLocation": false, "locationId": "8229", "stock": {"available": 0}},
                    "location": {"locationId": "8229"}, "distanceInKm": 16.6
                }]
            }]}
        }}}),
    )
    .await;
    mount_gql(
        &server,
        "getLocationDetail",
        json!({"data": {"locationQuery": {
            "publicName": "Sylvia Park Nz ",
            "address1": "286 Mt Wellington Highway ",
            "address2": "",
            "city": "Auckland",
            "state": "North Island",
            "latitude": "-36.914257999999997",
            "longitude": "174.841",
            "tradingHours": [{"weekDay": "Monday", "hours": "9:00am - 7:00pm"}]
        }}}),
    )
    .await;

    let stores = client(&server).stores_near("1010", 5).await.unwrap();
    assert_eq!(stores.len(), 1);
    let store = &stores[0];
    assert_eq!(store.name, "Sylvia Park Nz", "the gateway pads its names");
    assert_eq!(store.distance_km, Some(16.6), "carried from the probe");
    assert_eq!(
        store.address,
        ["286 Mt Wellington Highway"],
        "blanks dropped"
    );
    // Quoted in the payload, a number here.
    assert!(store.latitude.is_some_and(|v| v < -36.0));
    assert_eq!(store.hours.len(), 1);
}

#[tokio::test]
async fn an_account_call_with_no_token_says_to_sign_in_not_to_import_cookies() {
    let server = MockServer::start().await;
    // Admitted, so the challenge path is not what is being tested.
    let err = client(&server).wishlist().await.unwrap_err();
    assert!(matches!(err, kmart_api::Error::NotSignedIn), "{err:?}");
    assert_eq!(err.auth(), Some(net_kit::AuthFault::Missing));
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        0,
        "and it did not spend a request finding out"
    );
}

#[tokio::test]
async fn a_signed_out_cart_answers_null_rather_than_failing() {
    let server = MockServer::start().await;
    mount_gql(
        &server,
        "getMyActiveCart",
        json!({"data": {"me": {"activeCart": null}}}),
    )
    .await;

    let session = auth::session_from_netscape(&cookies_txt()).with_tokens(kmart_api::Tokens::new(
        "opaque".into(),
        None,
        Some(900),
    ));
    let cart = client_with(&server, session, Country::Nz)
        .cart()
        .await
        .unwrap();
    assert!(cart.is_none(), "a shopper who has not started one");
}

#[tokio::test]
async fn a_cart_change_addresses_the_line_by_its_own_id() {
    let server = MockServer::start().await;
    mount_gql(
        &server,
        "getMyActiveCart",
        json!({"data": {"me": {"activeCart": {
            "id": "cart-1",
            "version": 12,
            "totalPrice": {"centAmount": 3900},
            "lineItems": [{
                "id": "line-1",
                "name": "Milk Frother",
                "quantity": 1,
                "price": {"value": {"centAmount": 3900}},
                "totalPrice": {"centAmount": 3900},
                "variant": {"sku": "43165537", "attributes": []}
            }]
        }}}}),
    )
    .await;
    mount_gql(
        &server,
        "updateMyBag",
        json!({"data": {"updateMyCart": {
            "id": "cart-1",
            "version": 13,
            "totalPrice": {"centAmount": 7800},
            "lineItems": [{
                "id": "line-1",
                "name": "Milk Frother",
                "quantity": 2,
                "price": {"value": {"centAmount": 3900}},
                "totalPrice": {"centAmount": 7800},
                "variant": {"sku": "43165537", "attributes": []}
            }]
        }}}),
    )
    .await;

    let session = auth::session_from_netscape(&cookies_txt()).with_tokens(kmart_api::Tokens::new(
        "opaque".into(),
        None,
        Some(900),
    ));
    let cart = client_with(&server, session, Country::Nz)
        .cart_set("43165537", 2)
        .await
        .unwrap();
    assert_eq!(cart.version, 13);
    assert_eq!(cart.items(), 2);

    // The write must carry the line id and the version it just read; the
    // gateway refuses a stale one and the product's own keycode is not an
    // address it accepts.
    let sent = server.received_requests().await.unwrap();
    let update = sent
        .iter()
        .find(|r| String::from_utf8_lossy(&r.body).contains("updateMyBag"))
        .expect("an update was sent");
    let body: serde_json::Value = serde_json::from_slice(&update.body).unwrap();
    assert_eq!(body["variables"]["version"], 12);
    assert_eq!(
        body["variables"]["actions"][0]["changeLineItemQuantity"]["lineItemId"],
        "line-1"
    );
}

#[tokio::test]
async fn a_stale_version_is_reported_as_a_conflict_worth_retrying() {
    let server = MockServer::start().await;
    mount_gql(
        &server,
        "getMyActiveCart",
        json!({"data": {"me": {"activeCart": {
            "id": "cart-1", "version": 12,
            "lineItems": [{"id": "line-1", "name": "x", "quantity": 1,
                           "variant": {"sku": "43165537", "attributes": []}}]
        }}}}),
    )
    .await;
    mount_gql(
        &server,
        "updateMyBag",
        json!({"errors": [{
            "message": "Object cart-1 has a different version than expected",
            "extensions": {"code": "ConcurrentModification"}
        }]}),
    )
    .await;

    let session = auth::session_from_netscape(&cookies_txt()).with_tokens(kmart_api::Tokens::new(
        "opaque".into(),
        None,
        Some(900),
    ));
    let err = client_with(&server, session, Country::Nz)
        .cart_set("43165537", 2)
        .await
        .unwrap_err();
    assert!(matches!(err, kmart_api::Error::CartConflict), "{err:?}");
}

#[tokio::test]
async fn the_country_selects_the_index_and_the_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/search/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_body(json!([]), 0, 0)))
        .mount(&server)
        .await;
    mount_gql(
        &server,
        "getPostcodeSuggestions",
        json!({"data": {"postcodeQuery": []}}),
    )
    .await;

    let session = auth::session_from_netscape(&cookies_txt());
    let au = Client::new(
        http(),
        // The real key for Australia, not the mock default: the point is that
        // the country picks it.
        Endpoints::all(server.uri()).with_search_key(Country::Au.search_key()),
        Country::Au,
        session,
        "test-visitor",
    );
    au.search("mop", 1, 5).await.unwrap();
    au.postcodes("3000").await.unwrap();

    let sent = server.received_requests().await.unwrap();
    let search = sent
        .iter()
        .find(|r| r.url.path().starts_with("/search/"))
        .unwrap();
    assert!(
        search
            .url
            .query()
            .unwrap()
            .contains(Country::Au.search_key()),
        "the Australian index: {:?}",
        search.url.query()
    );
    let gql = sent
        .iter()
        .find(|r| r.url.path() == "/gateway/graphql")
        .unwrap();
    assert_eq!(
        gql.headers.get("x-country-code").unwrap().to_str().unwrap(),
        "AU"
    );
}

#[tokio::test]
async fn an_unparseable_body_keeps_the_status_and_a_snippet() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gateway/graphql"))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>"))
        .mount(&server)
        .await;

    let err = client(&server).postcodes("1010").await.unwrap_err();
    let text = format!("{err}");
    assert!(text.contains("502"), "{text}");
    assert!(!err.is_challenge(), "a real outage is not a challenge");
}

#[tokio::test]
async fn a_rotated_refresh_token_is_written_back_before_it_is_needed_again() {
    // Auth0 rotates on every use: the token that renewed this call is dead
    // afterwards, and the reply carries its replacement. Holding that only in
    // memory makes one command work and every later one fail with `invalid
    // grant` -- which reads like a wrong password and is not one.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "fresh-access",
            "refresh_token": "rotated",
            "expires_in": 900,
        })))
        .mount(&server)
        .await;
    mount_gql(
        &server,
        "getMyActiveCart",
        json!({"data": {"me": {"activeCart": null}}}),
    )
    .await;

    let dir = tempfile::TempDir::new().unwrap();
    let secrets = net_kit::Secrets::new("kmart-api-test", net_kit::Backend::File, dir.path());
    let session = auth::session_from_netscape(&cookies_txt())
        .with_tokens(kmart_api::Tokens::from_refresh("spent-once"));

    client_with(&server, session, Country::Nz)
        .with_session_store(Some(kmart_api::SessionStore {
            secrets: net_kit::Secrets::new("kmart-api-test", net_kit::Backend::File, dir.path()),
            email: Some("shopper@example.test".into()),
            auth_country: Some(Country::Nz),
        }))
        .cart()
        .await
        .unwrap();

    let stored = kmart_api::StoredSession::load(&secrets)
        .unwrap()
        .expect("the renewal was filed");
    let tokens = stored.tokens.clone().expect("with tokens");
    assert_eq!(tokens.refresh.as_deref(), Some("rotated"));
    assert_eq!(
        stored.email.as_deref(),
        Some("shopper@example.test"),
        "and saving did not drop who it belongs to"
    );
    assert_eq!(
        stored.cookies.len(),
        2,
        "nor the admission cookies it was carrying"
    );
    // Nor which Auth0 application minted it. Dropping this would renew fine
    // once and then pick the wrong client id on the run after, which is a
    // failure that looks nothing like its cause.
    assert_eq!(stored.auth_country(), Some(Country::Nz));
}

#[tokio::test]
async fn a_renewal_is_filed_without_a_password_to_fall_back_on() {
    // The saving and the fallback are separate things. Tying them together is
    // what broke this: a browser sign-in stores no password, so nothing was
    // ever written back.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "fresh-access",
            "refresh_token": "rotated",
            "expires_in": 900,
        })))
        .mount(&server)
        .await;
    mount_gql(
        &server,
        "getMyActiveCart",
        json!({"data": {"me": {"activeCart": null}}}),
    )
    .await;

    let dir = tempfile::TempDir::new().unwrap();
    let session = auth::session_from_netscape(&cookies_txt())
        .with_tokens(kmart_api::Tokens::from_refresh("spent-once"));
    let client = client_with(&server, session, Country::Nz)
        .with_session_store(Some(kmart_api::SessionStore {
            secrets: net_kit::Secrets::new("kmart-api-test", net_kit::Backend::File, dir.path()),
            email: None,
            auth_country: None,
        }))
        .with_reauth(None);

    client.cart().await.unwrap();

    let secrets = net_kit::Secrets::new("kmart-api-test", net_kit::Backend::File, dir.path());
    let stored = kmart_api::StoredSession::load(&secrets).unwrap().unwrap();
    assert_eq!(
        stored.tokens.and_then(|t| t.refresh).as_deref(),
        Some("rotated")
    );
}

#[tokio::test]
async fn removing_a_line_removes_it_rather_than_asking_for_none_of_it() {
    // `changeLineItemQuantity: 0` is the obvious spelling and the gateway
    // refuses it -- the quantity is validated as positive, so it answers
    // `VALIDATION_ERROR` and the line stays in the bag.
    let server = MockServer::start().await;
    mount_gql(
        &server,
        "getMyActiveCart",
        json!({"data": {"me": {"activeCart": {
            "id": "cart-1",
            "version": 12,
            "totalPrice": {"centAmount": 3900},
            "lineItems": [{
                "id": "line-1",
                "name": "Milk Frother",
                "quantity": 1,
                "price": {"value": {"centAmount": 3900}},
                "totalPrice": {"centAmount": 3900},
                "variant": {"sku": "43165537", "attributes": []}
            }]
        }}}}),
    )
    .await;
    mount_gql(
        &server,
        "updateMyBag",
        json!({"data": {"updateMyCart": {
            "id": "cart-1", "version": 13,
            "totalPrice": {"centAmount": 0}, "lineItems": []
        }}}),
    )
    .await;

    let session = auth::session_from_netscape(&cookies_txt()).with_tokens(kmart_api::Tokens::new(
        "opaque".into(),
        None,
        Some(900),
    ));
    // Both ways in: `cart remove`, and `cart set <keycode> 0`, which means the
    // same thing and must not be sent as a quantity.
    for quantity in [None, Some(0)] {
        let client = client_with(&server, session.clone(), Country::Nz);
        let cart = match quantity {
            None => client.cart_remove("43165537").await,
            Some(q) => client.cart_set("43165537", q).await,
        }
        .unwrap();
        assert!(cart.lines.is_empty());
    }

    for update in server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| String::from_utf8_lossy(&r.body).contains("updateMyBag"))
    {
        let body: serde_json::Value = serde_json::from_slice(&update.body).unwrap();
        assert_eq!(
            body["variables"]["actions"][0]["removeLineItem"]["lineItemId"],
            "line-1"
        );
    }
}
