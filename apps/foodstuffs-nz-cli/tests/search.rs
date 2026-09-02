//! End-to-end: search, specials, browse, and the client-side filters.
//!
//! Nothing here touches the internet -- both banners' endpoints are pointed at
//! a local server, which is also what makes the request bodies assertable.

mod support;

use serde_json::json;
use support::*;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn search_returns_normalised_products() {
    let f = Fixture::start().await;
    mount_search(
        &f.newworld,
        search_response(vec![
            product("5010819-EA-000", "Blue Milk", "Anchor", "2L", 450),
            special("5010820-EA-000", "Anchor Trim Milk", "Anchor", "2L", 399),
        ]),
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["--json", "search", "milk"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = stdout_json(&out);
    assert_eq!(json["banner"], "newworld");
    assert_eq!(json["store_id"], NW_STORE);
    assert_eq!(json["count"], 2);

    let first = &json["products"][0];
    assert_eq!(first["sku"], "5010819-EA-000");
    assert_eq!(first["brand"], "Anchor");
    assert_eq!(first["size"], "2L");
    assert_eq!(first["price"], json!(4.5), "cents should become dollars");
    assert_eq!(first["unit_price"], json!(2.25));
    assert_eq!(first["unit_measure"], "1L");
    assert_eq!(first["is_special"], false);
    assert_eq!(first["in_stock"], true);
    assert_eq!(first["department"], "Chilled, Frozen & Desserts");
    assert!(first["url"]
        .as_str()
        .unwrap()
        .ends_with("/shop/product/5010819-EA-000"));
    assert!(first["image"]
        .as_str()
        .unwrap()
        .contains("/200x200/5010819.png"));

    let second = &json["products"][1];
    assert_eq!(second["is_special"], true);
    assert_eq!(second["multi_buy"], "2 for $7.00");
}

#[tokio::test]
async fn search_renders_a_readable_listing() {
    let f = Fixture::start().await;
    mount_search(
        &f.newworld,
        search_response(vec![special("A-EA-000", "Blue Milk", "Anchor", "2L", 399)]),
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["search", "milk"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("New World"), "got: {stdout}");
    assert!(stdout.contains("Anchor Blue Milk"), "got: {stdout}");
    assert!(stdout.contains("$3.99 (special)"), "got: {stdout}");
    assert!(stdout.contains("$1.99 per 1L"), "got: {stdout}");
    assert!(stdout.contains("in stock"), "got: {stdout}");
}

#[tokio::test]
async fn search_scopes_the_query_to_the_selected_store() {
    let f = Fixture::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .and(body_partial_json(json!({
            "storeId": NW_STORE,
            "algoliaQuery": { "query": "milk", "filters": format!("stores:{NW_STORE}") }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_response(vec![])))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["search", "milk"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The mock's `expect(1)` is verified when the server drops.
}

#[tokio::test]
async fn specials_asks_only_for_promoted_products() {
    let f = Fixture::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .and(body_partial_json(json!({
            "algoliaQuery": {
                "query": "",
                "filters": format!("stores:{NW_STORE} AND onPromotion:{NW_STORE}")
            }
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(search_response(vec![special(
                "A-EA-000",
                "Blue Milk",
                "Anchor",
                "2L",
                399,
            )])),
        )
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--json", "specials"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout_json(&out)["count"], 1);
}

#[tokio::test]
async fn browse_filters_by_department() {
    let f = Fixture::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .and(body_partial_json(json!({
            "algoliaQuery": {
                "filters": format!("stores:{NW_STORE} AND category0NI:\"Fruit & Vegetables\"")
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_response(vec![])))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["browse", "Fruit & Vegetables"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn the_size_filter_narrows_results_after_fetching() {
    let f = Fixture::start().await;
    mount_search(
        &f.newworld,
        search_response(vec![
            product("A-EA-000", "Blue Milk", "Anchor", "2L", 450),
            product("B-EA-000", "Blue Milk", "Anchor", "600ml", 250),
        ]),
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["--json", "search", "milk", "--size", "600ml"])
        .output()
        .unwrap();

    let json = stdout_json(&out);
    assert_eq!(json["count"], 1);
    assert_eq!(json["products"][0]["sku"], "B-EA-000");
}

#[tokio::test]
async fn searching_without_a_store_explains_how_to_pick_one() {
    let f = Fixture::start().await;
    let out = f.cmd().args(["search", "milk"]).output().unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no New World store selected"),
        "got: {stderr}"
    );
    assert!(stderr.contains("store set"), "got: {stderr}");
}

#[tokio::test]
async fn an_empty_result_says_so_rather_than_printing_nothing() {
    let f = Fixture::start().await;
    mount_search(&f.newworld, search_response(vec![])).await;

    let out = f
        .cmd_with_stores()
        .args(["search", "durian"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("No products found for 'durian'"));
}
