//! The command surface itself: help, completions, doctor, and the shapes of
//! things that must work before any of the API does.

mod support;

use predicates::prelude::*;
use serde_json::json;
use support::{stdout_json, Fixture};

#[tokio::test]
async fn a_bare_invocation_prints_the_help() {
    let f = Fixture::start().await;

    f.cmd()
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Search products, specials and stores at Woolworths New Zealand.",
        ))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("Not affiliated with Woolworths"));
}

#[tokio::test]
async fn the_version_leads_with_the_version() {
    let f = Fixture::start().await;

    f.cmd()
        .arg("-V")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("wwnz 0."));
}

#[tokio::test]
async fn completions_are_generated_without_config_or_a_network() {
    let f = Fixture::start().await;

    // Deliberately pointed at nothing: generating a completion script must not
    // depend on the machine being set up, or on the API being reachable.
    f.cmd()
        .env("WWNZ_ORIGIN", "http://127.0.0.1:1")
        .env("WWNZ_CONFIG_DIR", "/nonexistent/config")
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("wwnz"));

    for shell in ["zsh", "fish"] {
        f.cmd()
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains("wwnz"));
    }
}

#[tokio::test]
async fn an_unknown_subcommand_is_refused() {
    let f = Fixture::start().await;

    f.cmd()
        .arg("teleport")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[tokio::test]
async fn the_sort_help_lists_what_the_api_accepts() {
    let f = Fixture::start().await;

    f.cmd()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("RELEVANCE"))
        .stdout(predicate::str::contains("CUP_PRICE_LOW_HIGH"));
}

#[tokio::test]
async fn doctor_reports_a_healthy_setup() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;

    f.cmd()
        .args(["doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok   guest access"))
        // Not being signed in is a note, not a failure: search still works.
        .stdout(predicate::str::contains("note account"));
}

#[tokio::test]
async fn doctor_fails_when_the_api_cannot_be_reached() {
    let f = Fixture::start().await;
    // Nothing is mounted for SearchLocations, so the call fails.

    f.cmd()
        .args(["doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("x    guest access"));
}

#[tokio::test]
async fn doctor_reports_a_signed_in_account() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_op(
        "Orders",
        json!({ "orders": { "results": [], "totalCount": 3, "totalPages": 1 } }),
    )
    .await;

    let out = f
        .cmd_signed_in()
        .args(["doctor", "--json"])
        .output()
        .expect("run");
    let json = stdout_json(&out);
    assert_eq!(json["healthy"], json!(true));
    let account = json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == json!("account"))
        .expect("an account check");
    assert_eq!(account["status"], json!("ok"));
}

#[tokio::test]
async fn a_storefront_that_hands_out_no_token_is_reported_clearly() {
    // A bot check: a 200 that sets no guest cookie. Without a token there is
    // nothing to call the API with, and the message has to say so. The bare
    // fixture leaves the storefront unmounted so this is the only answer.
    let f = Fixture::start_bare().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("<html>denied</html>"))
        .mount(&f.server)
        .await;

    f.cmd()
        .args(["search", "milk"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("set no __guest__token cookie"))
        .stderr(predicate::str::contains("WWNZ_GUEST_TOKEN"));
}
