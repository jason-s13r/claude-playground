//! End-to-end: listing stores and choosing the one to price against.
//!
//! Nothing here touches the internet -- both banners' endpoints are pointed at
//! a local server, which is also what makes the request bodies assertable.

mod support;

use support::*;

#[tokio::test]
async fn stores_lists_what_the_banner_returns() {
    let f = Fixture::start().await;
    let out = f.cmd().args(["--json", "stores"]).output().unwrap();
    assert!(out.status.success());

    let json = stdout_json(&out);
    assert_eq!(json["banner"], "newworld");
    assert_eq!(json["count"], 2);
    assert_eq!(json["stores"][0]["name"], "New World Thorndon");
}

#[tokio::test]
async fn stores_handles_the_wrapped_response_shape() {
    let f = Fixture::start().await;
    let out = f
        .cmd()
        .args(["--banner", "pns", "--json", "stores"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let json = stdout_json(&out);
    assert_eq!(json["count"], 1);
    assert_eq!(json["stores"][0]["id"], PNS_STORE);
}

#[tokio::test]
async fn stores_can_be_filtered_by_name() {
    let f = Fixture::start().await;
    let out = f
        .cmd()
        .args(["--json", "stores", "karori"])
        .output()
        .unwrap();

    let json = stdout_json(&out);
    assert_eq!(json["count"], 1);
    assert_eq!(json["stores"][0]["id"], "nw-store-2");
}

#[tokio::test]
async fn stores_can_be_found_by_town() {
    let f = Fixture::start().await;
    let out = f
        .cmd()
        .args(["--json", "stores", "wellington"])
        .output()
        .unwrap();

    // "Wellington" is the region, not part of any store's name.
    assert_eq!(stdout_json(&out)["count"], 2);
}

#[tokio::test]
async fn store_set_accepts_a_name_fragment_and_persists_it() {
    let f = Fixture::start().await;

    let out = f.cmd().args(["store", "set", "karori"]).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config = std::fs::read_to_string(f.home.path().join("config/config.toml")).unwrap();
    assert!(config.contains("nw-store-2"), "got: {config}");

    // And the saved choice is what a later command uses.
    let out = f.cmd().args(["--json", "store", "show"]).output().unwrap();
    assert_eq!(stdout_json(&out)["store"]["id"], "nw-store-2");
}

#[tokio::test]
async fn store_set_refuses_an_ambiguous_name() {
    let f = Fixture::start().await;
    let out = f
        .cmd()
        .args(["store", "set", "new world"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("matches 2"), "got: {stderr}");
    assert!(stderr.contains("nw-store-2"), "got: {stderr}");
}

#[tokio::test]
async fn store_set_rejects_a_store_the_banner_does_not_have() {
    let f = Fixture::start().await;
    let out = f
        .cmd()
        .args(["store", "set", "invercargill"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no New World store matches"));
}
