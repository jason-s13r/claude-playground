//! End-to-end: reading and changing the signed-in cart.
//!
//! Nothing here touches the internet -- both banners' endpoints are pointed at
//! a local server, which is also what makes the request bodies assertable.

mod support;

use serde_json::{json, Value};
use support::*;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---- cart ----------------------------------------------------------------

async fn mount_cart(server: &MockServer, items: Vec<Value>) {
    Mock::given(method("GET"))
        .and(path("/v1/edge/cart"))
        .respond_with(ResponseTemplate::new(200).set_body_json(cart_body(items.clone())))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/cart"))
        .respond_with(ResponseTemplate::new(200).set_body_json(cart_body(items)))
        .mount(server)
        .await;
}

#[tokio::test]
async fn cart_list_shows_the_lines_and_the_money() {
    let f = Fixture::start().await;
    mount_cart(
        &f.newworld,
        vec![
            cart_item("5039956-EA-000", "Broccoli", 1, "UNITS", 179),
            cart_item(
                "5101189-KGM-000",
                "NZ Premium Beef Mince",
                300,
                "WEIGHT",
                720,
            ),
        ],
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "cart", "list"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("New World Thorndon"), "got: {stdout}");
    assert!(stdout.contains("Broccoli"), "got: {stdout}");
    // Weight-priced lines are shown in grams, not as a bare number.
    assert!(stdout.contains("300g"), "got: {stdout}");
    assert!(stdout.contains("$1.79"), "got: {stdout}");
    assert!(stdout.contains("Bag fee"), "got: {stdout}");
    // 179 + 720 + 150 bag fee
    assert!(stdout.contains("$10.49"), "estimated total: {stdout}");
}

#[tokio::test]
async fn cart_list_says_so_when_empty() {
    let f = Fixture::start().await;
    mount_cart(&f.newworld, vec![]).await;
    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "cart", "list"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("The cart is empty"));
}

#[tokio::test]
async fn cart_add_infers_weight_from_the_sku() {
    let f = Fixture::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/cart"))
        .respond_with(ResponseTemplate::new(200).set_body_json(cart_body(vec![])))
        .mount(&f.newworld)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/cart"))
        .and(body_partial_json(json!({
            "products": [{ "productId": "5101189-KGM-000", "quantity": 300, "sale_type": "WEIGHT" }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(cart_body(vec![])))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args([
            "--token",
            &jwt(600),
            "cart",
            "add",
            "5101189-KGM-000",
            "300",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn cart_add_will_not_guess_how_much_loose_produce_you_want() {
    let f = Fixture::start().await;
    mount_cart(&f.newworld, vec![]).await;
    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "cart", "add", "5101189-KGM-000"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("sold by weight"), "got: {stderr}");
    assert!(stderr.contains("300"), "should show an example: {stderr}");
}

#[tokio::test]
async fn cart_add_tops_up_a_line_that_is_already_there() {
    let f = Fixture::start().await;
    // Two already in the cart; the API sets rather than increments, so adding
    // one more must send three.
    Mock::given(method("GET"))
        .and(path("/v1/edge/cart"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(cart_body(vec![cart_item(
                "5039956-EA-000",
                "Broccoli",
                2,
                "UNITS",
                358,
            )])),
        )
        .mount(&f.newworld)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/cart"))
        .and(body_partial_json(json!({
            "products": [{ "productId": "5039956-EA-000", "quantity": 3, "sale_type": "UNITS" }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(cart_body(vec![])))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "cart", "add", "5039956-EA-000"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn cart_remove_sets_the_quantity_to_zero() {
    let f = Fixture::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/cart"))
        .and(body_partial_json(json!({
            "products": [{ "productId": "5039956-EA-000", "quantity": 0 }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(cart_body(vec![])))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "cart", "remove", "5039956-EA-000"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn cart_clear_refuses_without_force() {
    let f = Fixture::start().await;
    mount_cart(&f.newworld, vec![]).await;
    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "cart", "clear"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));

    // Nothing was sent.
    let deletes = f
        .newworld
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method == wiremock::http::Method::DELETE)
        .count();
    assert_eq!(deletes, 0, "a refused clear must not touch the cart");
}

#[tokio::test]
async fn cart_clear_with_force_empties_it() {
    let f = Fixture::start().await;
    mount_cart(&f.newworld, vec![]).await;
    Mock::given(method("DELETE"))
        .and(path("/v1/edge/cart"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "cart", "clear", "--force"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn a_cart_without_an_account_points_at_login() {
    let f = Fixture::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/edge/cart"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorised"))
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "cart", "list"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("fsnz auth login"));
}

#[tokio::test]
async fn cart_json_carries_the_lines_and_the_totals() {
    let f = Fixture::start().await;
    mount_cart(
        &f.newworld,
        vec![cart_item(
            "5101189-KGM-000",
            "Beef Mince",
            300,
            "WEIGHT",
            720,
        )],
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "--json", "cart", "list"])
        .output()
        .unwrap();
    let json = stdout_json(&out);

    assert_eq!(json["items"][0]["sku"], "5101189-KGM-000");
    assert_eq!(json["items"][0]["sale_type"], "weight");
    assert_eq!(json["items"][0]["quantity"], 300);
    assert_eq!(json["items"][0]["line_total"], 720);
    assert_eq!(json["subtotal_cents"], 720);
    assert_eq!(json["store_name"], "New World Thorndon");
}
