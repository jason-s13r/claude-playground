//! End-to-end: reading order history.
//!
//! Both banners' endpoints point at a local server, so the request paths and
//! bodies -- which is what tells an in-store order from an online one -- are
//! assertable without touching the network.

mod support;

use serde_json::{json, Value};
use support::*;
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The id shape a till receipt has: a path, not an opaque token.
const INSTORE_ID: &str =
    "region/fsni/banner/NW/customer/1234567890/salesstaginglink/_S_000001234_D_20260801";

fn order_row(id: &str, cents: i64, at: &str, source: &str) -> Value {
    json!({
        "orderId": id,
        "amount": cents,
        "orderTimestamp": at,
        "store": { "name": "New World Thorndon", "id": NW_STORE, "region": "NI" },
        "source": source,
    })
}

async fn mount_orders(server: &MockServer, orders: Vec<Value>) {
    let total = orders.len();
    Mock::given(method("GET"))
        .and(path("/v1/edge/order/paged"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "pageInfo": {
                "pageNumber": 1,
                "currentPageContentsCount": total,
                "totalPages": 1,
                "totalContentsCount": total,
                "requestedSize": 20,
            },
            "orders": orders,
        })))
        .mount(server)
        .await;
}

async fn mount_instore_order(server: &MockServer, id: &str, products: Vec<Value>) {
    Mock::given(method("GET"))
        .and(path("/v1/edge/order/instore"))
        .and(query_param("orderId", id))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "summary": order_row(id, 1486, "2026-08-01T16:00:00+12:00", "IN_STORE"),
            "products": products,
        })))
        .expect(1)
        .mount(server)
        .await;
}

fn instore_product(sku: &str, name: &str, brand: &str, qty: u32, cents: i64) -> Value {
    json!({
        "productId": sku, "quantity": qty, "sale_type": "UNITS",
        "price": cents, "name": name, "brand": brand,
        "categories": ["Milk"], "gtin": "94145953",
    })
}

#[tokio::test]
async fn orders_list_numbers_the_history() {
    let f = Fixture::start().await;
    mount_orders(
        &f.newworld,
        vec![
            order_row(
                INSTORE_ID,
                1486,
                "2026-08-01T16:00:00+12:00",
                "IN_STORE",
            ),
            order_row("9f1c", 4200, "2026-07-01T16:00:00+12:00", "ONLINE"),
        ],
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "list"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("New World Thorndon"), "got: {stdout}");
    assert!(stdout.contains("2026-08-01 16:00"), "got: {stdout}");
    assert!(stdout.contains("$14.86"), "got: {stdout}");
    assert!(stdout.contains("in store"), "got: {stdout}");
    assert!(stdout.contains("online"), "got: {stdout}");
    // The ids are unusable by hand, so the listing is numbered and says so.
    assert!(stdout.contains("fsnz orders show <#>"), "got: {stdout}");
    assert!(
        !stdout.contains(INSTORE_ID),
        "the id is JSON-only: {stdout}"
    );
}

#[tokio::test]
async fn orders_list_asks_the_api_for_one_kind_when_told_to() {
    let f = Fixture::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/order/paged"))
        .and(query_param("source", "ONLINE"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "pageInfo": { "totalPages": 0, "totalContentsCount": 0 },
            "orders": [],
        })))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "list", "--source", "online"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("No online orders"));
}

#[tokio::test]
async fn orders_list_defaults_to_every_kind() {
    let f = Fixture::start().await;
    mount_orders(&f.newworld, vec![]).await;
    f.cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "list"])
        .output()
        .unwrap();

    let requests = f.newworld.received_requests().await.unwrap();
    let paged = requests
        .iter()
        .find(|r| r.url.path() == "/v1/edge/order/paged")
        .expect("the list was fetched");
    assert!(
        paged.url.query().unwrap_or_default().contains("source=ALL"),
        "got: {:?}",
        paged.url.query()
    );
}

#[tokio::test]
async fn orders_show_takes_a_position_from_the_listing() {
    let f = Fixture::start().await;
    mount_orders(
        &f.newworld,
        vec![order_row(
            INSTORE_ID,
            1486,
            "2026-08-01T16:00:00+12:00",
            "IN_STORE",
        )],
    )
    .await;
    mount_instore_order(
        &f.newworld,
        INSTORE_ID,
        vec![
            instore_product("5011234-EA-000", "Creamy Milk Chocolate Block", "Whittaker's", 2, 774),
            instore_product("5019876-EA-000", "Wholegrain Toast Bread", "Pams", 2, 712),
        ],
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "show", "1"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("Whittaker's Creamy Milk"),
        "got: {stdout}"
    );
    assert!(stdout.contains("5019876-EA-000"), "got: {stdout}");
    assert!(stdout.contains("$7.74"), "got: {stdout}");
    assert!(stdout.contains("Total"), "got: {stdout}");
    assert!(stdout.contains("$14.86"), "got: {stdout}");
    // The lines add up to the total, so there is no separate Items row.
    assert!(!stdout.contains("Items"), "got: {stdout}");
    assert!(stdout.contains("2 lines"), "got: {stdout}");
}

#[tokio::test]
async fn orders_show_routes_a_pasted_id_by_its_shape() {
    let f = Fixture::start().await;
    // No listing is mounted: a whole id must not need one looked up.
    mount_instore_order(
        &f.newworld,
        INSTORE_ID,
        vec![instore_product(
            "5011234-EA-000",
            "Creamy Milk Chocolate Block",
            "Anchor",
            2,
            1486,
        )],
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "show", INSTORE_ID])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let requests = f.newworld.received_requests().await.unwrap();
    assert!(
        !requests
            .iter()
            .any(|r| r.url.path() == "/v1/edge/order/paged"),
        "an id should not need the listing"
    );
    // The site sends the id's slashes unescaped, and so does this: an
    // undocumented endpoint is likelier to accept what it already sees.
    let detail = requests
        .iter()
        .find(|r| r.url.path() == "/v1/edge/order/instore")
        .expect("the order was fetched");
    assert_eq!(
        detail.url.query(),
        Some(format!("orderId={INSTORE_ID}").as_str())
    );
}

#[tokio::test]
async fn an_online_id_goes_to_the_other_endpoint() {
    let f = Fixture::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/order/9f1c"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "summary": {
                "orderId": "9f1c",
                "orderAmountInCents": 4200,
                "status": "DELIVERED",
                "storeName": "New World Thorndon",
                "deliveryAddress": "1 Molesworth St, Wellington",
                "timeslot": { "date": "2026-08-01T00:00:00+12:00", "slot": "10:00 - 12:00" },
                "serviceFee": 500,
                "bagFee": 150,
                "source": "ONLINE",
            },
            "products": [{
                "productId": "5039956-EA-000", "name": "Broccoli",
                "quantity": 1, "saleType": "UNITS", "price": 3550,
            }],
        })))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "show", "9f1c"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("DELIVERED"), "got: {stdout}");
    assert!(stdout.contains("10:00 - 12:00"), "got: {stdout}");
    assert!(stdout.contains("1 Molesworth St"), "got: {stdout}");
    assert!(stdout.contains("Service fee"), "got: {stdout}");
    // Fees mean the lines no longer add up to the total, so both are shown.
    assert!(stdout.contains("Items"), "got: {stdout}");
    assert!(stdout.contains("$35.50"), "got: {stdout}");
    assert!(stdout.contains("$42.00"), "got: {stdout}");
}

#[tokio::test]
async fn a_position_past_the_end_says_how_many_there_are() {
    let f = Fixture::start().await;
    mount_orders(&f.newworld, vec![]).await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "show", "9"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no order 9"), "got: {stderr}");
    assert!(stderr.contains("0 orders"), "got: {stderr}");
}

#[tokio::test]
async fn previous_purchases_ask_for_what_is_not_already_in_the_cart() {
    let f = Fixture::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/order/previousPurchases"))
        .and(body_partial_json(
            json!({ "excludeCart": true, "maximumResults": 20 }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "products": [
                {
                    "productId": "5101234-KGM-000", "name": "Whole Almonds",
                    "quantity": 1000, "price": 4000, "saleType": "WEIGHT", "isCatered": false,
                },
                {
                    "productId": "5019876-EA-000", "name": "Wholegrain Toast Bread",
                    "quantity": 1, "price": 499, "saleType": "UNITS", "brand": "Pams",
                },
            ],
        })))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "previous"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("Whole Almonds"), "got: {stdout}");
    // Weight-priced lines read in kilos, as everywhere else.
    assert!(stdout.contains("1kg"), "got: {stdout}");
    assert!(stdout.contains("Pams Wholegrain"), "got: {stdout}");
    assert!(stdout.contains("fsnz cart add"), "got: {stdout}");
}

#[tokio::test]
async fn include_cart_asks_for_those_too() {
    let f = Fixture::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/order/previousPurchases"))
        .and(body_partial_json(json!({ "excludeCart": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "products": [] })))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "previous", "--include-cart"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn history_without_an_account_points_at_login() {
    let f = Fixture::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/order/paged"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorised"))
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "orders", "list"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("order history needs an account"),
        "got: {stderr}"
    );
    assert!(stderr.contains("fsnz auth login"), "got: {stderr}");
}

#[tokio::test]
async fn orders_json_carries_the_ids_and_the_totals() {
    let f = Fixture::start().await;
    mount_orders(
        &f.newworld,
        vec![order_row(
            INSTORE_ID,
            1486,
            "2026-08-01T16:00:00+12:00",
            "IN_STORE",
        )],
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "--json", "orders", "list"])
        .output()
        .unwrap();
    let json = stdout_json(&out);

    assert_eq!(json["count"], 1);
    assert_eq!(json["total_available"], 1);
    assert_eq!(json["orders"][0]["id"], INSTORE_ID);
    assert_eq!(json["orders"][0]["source"], "in-store");
    assert_eq!(json["orders"][0]["total"], 14.86);
    assert_eq!(json["orders"][0]["store_name"], "New World Thorndon");
}
