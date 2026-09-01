//! `wwnz cart`.

mod support;

use predicates::prelude::*;
use serde_json::json;
use support::{cart_body, cart_line, stdout_json, Fixture};

#[tokio::test]
async fn cart_list_renders_the_lines_and_the_totals() {
    let f = Fixture::start().await;
    f.mount_op(
        "CustomerCart",
        json!({ "customerCart": cart_body(vec![
            cart_line("282768", "Milk Standard 3L", "Woolworths", 1, 719),
            cart_line("282765", "Milk Standard 2L", "Woolworths", 2, 482),
        ])}),
    )
    .await;

    f.cmd_signed_in()
        .args(["cart", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Woolworths Milk Standard 3L"))
        .stdout(predicate::str::contains("$7.19"))
        // The 2L line is two at $4.82.
        .stdout(predicate::str::contains("$9.64"))
        // The rows add up to Items; Fees is what closes the gap to To pay.
        .stdout(predicate::str::contains("Items:    $16.83"))
        .stdout(predicate::str::contains("Fees:     $5.00"))
        .stdout(predicate::str::contains("To pay:   $21.83"))
        // Quantities and products are counted separately, and both are said.
        .stdout(predicate::str::contains("3 items across 2 products"))
        .stdout(predicate::str::contains("Regent Woolworths (pickup)"));
}

#[tokio::test]
async fn an_empty_cart_says_so() {
    let f = Fixture::start().await;
    f.mount_op("CustomerCart", json!({ "customerCart": cart_body(vec![]) }))
        .await;

    f.cmd_signed_in()
        .args(["cart", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("The cart is empty."));
}

#[tokio::test]
async fn the_cart_needs_an_account() {
    let f = Fixture::start().await;

    f.cmd()
        .args(["cart", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("this needs an account"))
        .stderr(predicate::str::contains("wwnz auth login"));
}

#[tokio::test]
async fn a_rejected_session_points_at_signing_in_again() {
    let f = Fixture::start().await;
    f.mount_op_error(
        "CustomerCart",
        "The current user is not authorized to access this resource.",
        "AUTH_NOT_AUTHENTICATED",
    )
    .await;

    f.cmd_signed_in()
        .args(["cart", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("the cart needs an account"))
        .stderr(predicate::str::contains("wwnz auth login"));
}

#[tokio::test]
async fn adding_is_relative_to_what_is_already_there() {
    let f = Fixture::start().await;
    // The cart already holds one; adding two must ask for three, not two.
    f.mount_op(
        "CustomerCart",
        json!({ "customerCart": cart_body(vec![cart_line(
            "282768", "Milk Standard 3L", "Woolworths", 1, 719
        )])}),
    )
    .await;
    f.mount_op(
        "SetCartLineItemQuantity",
        json!({ "setCartLineItemQuantity": cart_body(vec![cart_line(
            "282768", "Milk Standard 3L", "Woolworths", 3, 719
        )])}),
    )
    .await;

    f.cmd_signed_in()
        .args(["cart", "add", "282768", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("282768-EA × 1 → × 3"));

    // The quantity actually sent is what proves the arithmetic happened.
    let requests = f.server.received_requests().await.expect("recording is on");
    let sent = requests
        .iter()
        .filter(|r| {
            r.url
                .query()
                .is_some_and(|q| q.contains("SetCartLineItemQuantity"))
        })
        .map(|r| serde_json::from_slice::<serde_json::Value>(&r.body).unwrap())
        .next()
        .expect("a quantity was set");
    assert_eq!(
        sent["variables"]["input"]["cartLineItemQuantityUpdates"][0],
        json!({ "variantKey": "282768-EA", "quantity": 3 })
    );
}

#[tokio::test]
async fn update_sets_the_quantity_outright() {
    let f = Fixture::start().await;
    f.mount_op(
        "CustomerCart",
        json!({ "customerCart": cart_body(vec![cart_line(
            "282768", "Milk Standard 3L", "Woolworths", 5, 719
        )])}),
    )
    .await;
    f.mount_op(
        "SetCartLineItemQuantity",
        json!({ "setCartLineItemQuantity": cart_body(vec![cart_line(
            "282768", "Milk Standard 3L", "Woolworths", 2, 719
        )])}),
    )
    .await;

    f.cmd_signed_in()
        .args(["cart", "update", "282768", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("282768-EA × 5 → × 2"));
}

#[tokio::test]
async fn removing_a_line_sets_it_to_zero() {
    let f = Fixture::start().await;
    f.mount_op(
        "CustomerCart",
        json!({ "customerCart": cart_body(vec![cart_line(
            "282768", "Milk Standard 3L", "Woolworths", 2, 719
        )])}),
    )
    .await;
    f.mount_op(
        "SetCartLineItemQuantity",
        json!({ "setCartLineItemQuantity": cart_body(vec![]) }),
    )
    .await;

    f.cmd_signed_in()
        .args(["cart", "remove", "282768"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed 282768-EA"));

    let requests = f.server.received_requests().await.expect("recording is on");
    let sent = requests
        .iter()
        .filter(|r| {
            r.url
                .query()
                .is_some_and(|q| q.contains("SetCartLineItemQuantity"))
        })
        .map(|r| serde_json::from_slice::<serde_json::Value>(&r.body).unwrap())
        .next()
        .expect("a quantity was set");
    // There is no delete on this API; zero is how a line goes away.
    assert_eq!(
        sent["variables"]["input"]["cartLineItemQuantityUpdates"][0]["quantity"],
        json!(0)
    );
}

#[tokio::test]
async fn removing_something_absent_is_refused_before_any_call() {
    let f = Fixture::start().await;
    f.mount_op("CustomerCart", json!({ "customerCart": cart_body(vec![]) }))
        .await;

    f.cmd_signed_in()
        .args(["cart", "remove", "999999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("999999-EA is not in the cart"));
}

#[tokio::test]
async fn clearing_the_cart_requires_force() {
    let f = Fixture::start().await;
    f.mount_op("ClearCart", json!({ "clearCart": cart_body(vec![]) }))
        .await;

    f.cmd_signed_in()
        .args(["cart", "clear"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pass --force"));

    f.cmd_signed_in()
        .args(["cart", "clear", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cart emptied"));
}

#[tokio::test]
async fn a_failed_validation_is_surfaced_as_a_warning() {
    let f = Fixture::start().await;
    let mut body = cart_body(vec![cart_line(
        "282768",
        "Milk Standard 3L",
        "Woolworths",
        1,
        719,
    )]);
    body["validationResult"] = json!({
        "isValid": false,
        "failedValidations": [{
            "ruleName": "STOCK", "title": "Out of stock",
            "message": "Milk Standard 3L is out of stock at this store",
            "affectedSkus": ["282768"],
        }],
    });
    f.mount_op("CustomerCart", json!({ "customerCart": body }))
        .await;

    f.cmd_signed_in()
        .args(["cart", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "warning: Milk Standard 3L is out of stock",
        ));
}

#[tokio::test]
async fn the_unit_flag_replaces_the_variant_suffix() {
    let f = Fixture::start().await;
    f.mount_op("CustomerCart", json!({ "customerCart": cart_body(vec![]) }))
        .await;
    f.mount_op(
        "SetCartLineItemQuantity",
        json!({ "setCartLineItemQuantity": cart_body(vec![]) }),
    )
    .await;

    f.cmd_signed_in()
        .args(["cart", "add", "133211", "500", "--unit", "kgm"])
        .assert()
        .success()
        .stdout(predicate::str::contains("added 133211-KGM × 500"));
}

#[tokio::test]
async fn json_output_carries_the_cart_and_what_changed() {
    let f = Fixture::start().await;
    f.mount_op(
        "CustomerCart",
        json!({ "customerCart": cart_body(vec![cart_line(
            "282768", "Milk Standard 3L", "Woolworths", 1, 719
        )])}),
    )
    .await;

    let out = f
        .cmd_signed_in()
        .args(["cart", "list", "--json"])
        .output()
        .expect("run");
    let json = stdout_json(&out);
    assert_eq!(json["total_items"], json!(1));
    assert_eq!(json["items"], json!(7.19), "the lines alone");
    assert_eq!(json["subtotal"], json!(12.19), "lines plus the pickup fee");
    assert_eq!(json["lines"][0]["sku"], json!("282768"));
    assert_eq!(json["lines"][0]["unit_price"], json!(7.19));
}
