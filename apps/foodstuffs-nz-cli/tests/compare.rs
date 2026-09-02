//! End-to-end: the same search run against both banners at once.
//!
//! Nothing here touches the internet -- both banners' endpoints are pointed at
//! a local server, which is also what makes the request bodies assertable.

mod support;

use serde_json::json;
use support::*;

#[tokio::test]
async fn compare_puts_the_two_banners_side_by_side() {
    let f = Fixture::start().await;
    mount_search(
        &f.newworld,
        search_response(vec![
            product("SHARED-EA-000", "Blue Milk", "Anchor", "2L", 450),
            product("NW-ONLY-EA-000", "Fancy Cheese", "Kapiti", "120g", 999),
        ]),
    )
    .await;
    mount_search(
        &f.paknsave,
        search_response(vec![product(
            "SHARED-EA-000",
            "Blue Milk",
            "Anchor",
            "2L",
            399,
        )]),
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["--json", "compare", "milk"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = stdout_json(&out);
    assert_eq!(json["banners"], json!(["newworld", "paknsave"]));
    assert_eq!(json["count"], 2);

    // Products both banners stock sort first.
    let shared = &json["rows"][0];
    assert_eq!(shared["found_at_both"], true);
    assert_eq!(shared["cheapest"], "paknsave");
    assert_eq!(shared["difference"], json!(0.51));
    assert_eq!(shared["banners"]["newworld"]["price"], json!(4.5));
    assert_eq!(shared["banners"]["paknsave"]["price"], json!(3.99));

    let unmatched = &json["rows"][1];
    assert_eq!(unmatched["found_at_both"], false);
    assert!(unmatched["banners"]["paknsave"].is_null());
}

#[tokio::test]
async fn compare_renders_a_table_marking_the_cheaper_banner() {
    let f = Fixture::start().await;
    mount_search(
        &f.newworld,
        search_response(vec![product("S-EA-000", "Blue Milk", "Anchor", "2L", 450)]),
    )
    .await;
    mount_search(
        &f.paknsave,
        search_response(vec![product("S-EA-000", "Blue Milk", "Anchor", "2L", 399)]),
    )
    .await;

    let out = f
        .cmd_with_stores()
        .args(["compare", "milk"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("New World"), "got: {stdout}");
    assert!(stdout.contains("PAK'nSAVE"), "got: {stdout}");
    assert!(stdout.contains("$0.51"), "difference column: {stdout}");
    assert!(stdout.contains("1 found at both"), "got: {stdout}");
}
