//! `wwnz orders`.

mod support;

use predicates::prelude::*;
use serde_json::json;
use support::{order, product, stdout_json, Fixture};

#[tokio::test]
async fn orders_list_renders_the_history() {
    let f = Fixture::start().await;
    f.mount_op(
        "Orders",
        json!({ "orders": {
            "results": [
                order("12345678", "2026-09-02T14:30:00+12:00", 2518),
                order("12345677", "2026-08-20T09:00:00+12:00", 8140),
            ],
            "totalCount": 2, "totalPages": 1,
        }}),
    )
    .await;

    f.cmd_signed_in()
        .args(["orders", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("12345678"))
        // The timestamp is trimmed to the date a table row has room for.
        .stdout(predicate::str::contains("2026-09-02"))
        .stdout(predicate::str::contains("pickup — Regent Woolworths"))
        .stdout(predicate::str::contains("$25.18"))
        .stdout(predicate::str::contains("2 orders"));
}

#[tokio::test]
async fn an_account_with_no_orders_says_so() {
    let f = Fixture::start().await;
    f.mount_op(
        "Orders",
        json!({ "orders": { "results": [], "totalCount": 0, "totalPages": 0 } }),
    )
    .await;

    f.cmd_signed_in()
        .args(["orders", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No orders."));
}

#[tokio::test]
async fn orders_need_an_account() {
    let f = Fixture::start().await;

    f.cmd()
        .args(["orders", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("this needs an account"));
}

#[tokio::test]
async fn the_filter_reaches_the_api_as_the_value_it_expects() {
    let f = Fixture::start().await;
    f.mount_op(
        "Orders",
        json!({ "orders": { "results": [], "totalCount": 0, "totalPages": 0 } }),
    )
    .await;

    f.cmd_signed_in()
        .args(["orders", "list", "--filter", "past"])
        .assert()
        .success();

    let requests = f.server.received_requests().await.expect("recording is on");
    let sent = requests
        .iter()
        .filter(|r| r.url.query().is_some_and(|q| q.contains("Orders")))
        .map(|r| serde_json::from_slice::<serde_json::Value>(&r.body).unwrap())
        .next()
        .expect("orders were asked for");
    assert_eq!(sent["variables"]["input"]["inclusiveFilter"], json!("PAST"));
}

#[tokio::test]
async fn an_unknown_filter_is_rejected_before_any_call() {
    let f = Fixture::start().await;

    f.cmd_signed_in()
        .args(["orders", "list", "--filter", "sideways"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown order filter"));
}

#[tokio::test]
async fn previous_purchases_render_as_a_product_list() {
    let f = Fixture::start().await;
    // "Buy it again" is a product search with an account-scoped selector, so
    // it comes back as products rather than as order history.
    f.mount_search(
        vec![product("282768", "Milk Standard 3L", "Woolworths", 7.19)],
        1,
    )
    .await;

    f.cmd_signed_in()
        .args(["orders", "previous"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Woolworths Milk Standard 3L"))
        .stdout(predicate::str::contains("SKU: 282768"));

    let requests = f.server.received_requests().await.expect("recording is on");
    let sent = requests
        .iter()
        .filter(|r| r.url.query().is_some_and(|q| q.contains("ProductSearch")))
        .map(|r| serde_json::from_slice::<serde_json::Value>(&r.body).unwrap())
        .next()
        .expect("a search was made");
    let input = &sent["variables"]["searchInput"];
    assert!(input["byBuyAgain"].is_object(), "got: {input}");
    // Frequency is the only ordering that means anything here.
    assert_eq!(input["byBuyAgain"]["sortBy"], json!("FREQUENCY"));
}

#[tokio::test]
async fn json_output_carries_the_orders() {
    let f = Fixture::start().await;
    f.mount_op(
        "Orders",
        json!({ "orders": {
            "results": [order("12345678", "2026-09-02T14:30:00+12:00", 2518)],
            "totalCount": 1, "totalPages": 1,
        }}),
    )
    .await;

    let out = f
        .cmd_signed_in()
        .args(["orders", "list", "--json"])
        .output()
        .expect("run");
    let json = stdout_json(&out);
    assert_eq!(json["count"], json!(1));
    assert_eq!(json["filter"], json!("all"));
    assert_eq!(json["orders"][0]["number"], json!("12345678"));
    assert_eq!(json["orders"][0]["total"], json!(25.18));
}
