//! `wwnz stores` and `wwnz store`.

mod support;

use predicates::prelude::*;
use serde_json::json;
use support::{location, stdout_json, Fixture, STORE_ID, STORE_NAME};

#[tokio::test]
async fn stores_lists_what_the_api_returns() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;

    f.cmd()
        .args(["stores", "whangarei"])
        .assert()
        .success()
        .stdout(predicate::str::contains(STORE_NAME))
        .stdout(predicate::str::contains("Whangarei Woolworths"))
        .stdout(predicate::str::contains("9048"))
        .stdout(predicate::str::contains("0.6 km"));
}

#[tokio::test]
async fn a_store_search_with_no_matches_says_so() {
    let f = Fixture::start().await;
    f.mount_stores(json!([])).await;

    f.cmd()
        .args(["stores", "atlantis"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No store matches 'atlantis'."));
}

#[tokio::test]
async fn setting_a_store_saves_it_and_show_reports_it() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;

    f.cmd()
        .args(["store", "set", "regent"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Pricing against Regent Woolworths (9048)",
        ));

    // The name is saved alongside the id, so `show` needs no round trip.
    f.cmd()
        .args(["store", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Pricing against Regent Woolworths (9048)",
        ));
}

#[tokio::test]
async fn an_ambiguous_store_name_is_refused_with_the_candidates() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;

    // Both fixture stores are in Whangarei, so the town alone cannot choose.
    f.cmd()
        .args(["store", "set", "whangarei"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("matches 2 stores"))
        .stderr(predicate::str::contains("9048"))
        .stderr(predicate::str::contains("9195"))
        .stderr(predicate::str::contains("Use an id."));
}

#[tokio::test]
async fn an_exact_id_wins_over_an_ambiguous_name() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;

    f.cmd()
        .args(["store", "set", "9195"])
        .assert()
        .success()
        .stdout(predicate::str::contains("9195"));
}

#[tokio::test]
async fn setting_a_store_that_does_not_exist_is_refused() {
    let f = Fixture::start().await;
    f.mount_stores(json!([location("9057", "Ponsonby Woolworths", "Ponsonby")]))
        .await;

    f.cmd()
        .args(["store", "set", "atlantis"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no store matches 'atlantis'"));
}

#[tokio::test]
async fn clearing_a_store_forgets_it() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;

    f.cmd().args(["store", "set", "regent"]).assert().success();
    f.cmd().args(["store", "clear"]).assert().success();

    let out = f
        .cmd()
        .args(["store", "show", "--json"])
        .output()
        .expect("run");
    assert_eq!(stdout_json(&out)["store"], json!(null));
}

#[tokio::test]
async fn show_reports_nothing_selected_rather_than_failing() {
    let f = Fixture::start().await;

    f.cmd()
        .args(["store", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No store selected"));
}

#[tokio::test]
async fn the_store_flag_overrides_the_saved_one() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.cmd().args(["store", "set", "regent"]).assert().success();

    let out = f
        .cmd()
        .args(["--store", "9195", "store", "show", "--json"])
        .output()
        .expect("run");
    assert_eq!(stdout_json(&out)["store"]["id"], json!("9195"));

    // And the saved one is untouched by having been overridden once.
    let out = f
        .cmd()
        .args(["store", "show", "--json"])
        .output()
        .expect("run");
    assert_eq!(stdout_json(&out)["store"]["id"], json!(STORE_ID));
}
