//! `wwnz search`, `specials`, `browse` and `departments`.

mod support;

use predicates::prelude::*;
use serde_json::json;
use support::{ad_slot, product, product_priced, special, stdout_json, Fixture};

#[tokio::test]
async fn search_prints_products_grouped_by_name() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_search(
        vec![
            product_priced("282768", "Milk Standard 3L", "Woolworths", 7.19, 2.40),
            product("282765", "Milk Standard 2L", "Woolworths", 4.82),
        ],
        2,
    )
    .await;

    f.cmd_with_store()
        .args(["search", "milk"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Woolworths Milk Standard 3L"))
        .stdout(predicate::str::contains("$7.19"))
        // Per-unit pricing comes from the cup price, not the selling price.
        .stdout(predicate::str::contains("$2.40 per 1L"))
        .stdout(predicate::str::contains("SKU: 282768"))
        .stdout(predicate::str::contains("in stock"));
}

#[tokio::test]
async fn a_special_shows_the_old_price_and_the_saving() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_search(
        vec![special(
            "6022439",
            "Deodorant Roll On 50mL",
            "Dove",
            5.99,
            6.99,
        )],
        1,
    )
    .await;

    f.cmd_with_store()
        .args(["specials"])
        .assert()
        .success()
        .stdout(predicate::str::contains("$5.99 (was $6.99, save $1.00)"));
}

#[tokio::test]
async fn ad_slots_in_the_results_are_skipped_rather_than_rendered() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    // The real search interleaves ad slots with products. They are a different
    // union member with none of a product's fields, and must not become rows.
    f.mount_search(
        vec![
            ad_slot(),
            product("282768", "Milk Standard 3L", "Woolworths", 7.19),
            ad_slot(),
        ],
        1,
    )
    .await;

    let out = f
        .cmd_with_store()
        .args(["search", "milk", "--json"])
        .output()
        .expect("run");
    let json = stdout_json(&out);
    assert_eq!(json["count"], json!(1), "only the product should survive");
    assert_eq!(json["products"][0]["sku"], json!("282768"));
}

#[tokio::test]
async fn a_sponsored_product_is_kept_but_marked() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    let mut sponsored = product("282848", "Milk Standard Blue 1L", "Anchor", 3.73);
    sponsored["__typename"] = json!("SponsoredProduct");
    f.mount_search(vec![sponsored], 1).await;

    let out = f
        .cmd_with_store()
        .args(["search", "milk", "--json"])
        .output()
        .expect("run");
    let json = stdout_json(&out);
    assert_eq!(json["count"], json!(1));
    assert_eq!(json["products"][0]["sponsored"], json!(true));
}

#[tokio::test]
async fn the_size_filter_narrows_the_results_client_side() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_search(
        vec![
            product("282768", "Milk Standard 3L", "Woolworths", 7.19),
            product("282765", "Milk Standard 2L", "Woolworths", 4.82),
        ],
        2,
    )
    .await;

    let out = f
        .cmd_with_store()
        .args(["search", "milk", "--size", "2L", "--json"])
        .output()
        .expect("run");
    let json = stdout_json(&out);
    assert_eq!(json["count"], json!(1));
    assert_eq!(json["products"][0]["sku"], json!("282765"));
}

#[tokio::test]
async fn a_search_with_no_results_says_so_rather_than_printing_nothing() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_search(vec![], 0).await;

    f.cmd_with_store()
        .args(["search", "gold-plated caviar"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No products found for 'gold-plated caviar'.",
        ));
}

#[tokio::test]
async fn browse_resolves_a_department_name_to_its_key() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_categories().await;
    f.mount_search(
        vec![product("133211", "Fresh Bananas", "Woolworths", 3.75)],
        1,
    )
    .await;

    f.cmd_with_store()
        .args(["browse", "fruit"])
        .assert()
        .success()
        // Which department was chosen has to be visible: "fruit" could
        // reasonably match more than one thing.
        .stdout(predicate::str::contains("Browsing Fruit & Veg (9-VEG)"))
        .stdout(predicate::str::contains("Fresh Bananas"));
}

#[tokio::test]
async fn browse_refuses_a_department_it_cannot_find() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_categories().await;

    f.cmd_with_store()
        .args(["browse", "charcuterie"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no department matches 'charcuterie'",
        ))
        .stderr(predicate::str::contains("wwnz departments"));
}

#[tokio::test]
async fn departments_lists_the_tree_to_the_requested_depth() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_categories().await;

    // Depth 1 is departments only: the aisle inside Fruit & Veg is below it.
    f.cmd()
        .args(["departments", "--depth", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fruit & Veg"))
        .stdout(predicate::str::contains("Bakery"))
        .stdout(predicate::str::contains("Apples").not());

    f.cmd()
        .args(["departments", "--depth", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Apples"));
}

#[tokio::test]
async fn search_works_without_a_store_selected() {
    let f = Fixture::start().await;
    // No store selection is mounted: with none chosen the site prices against
    // a default, and the command must not require one first.
    f.mount_search(
        vec![product("282768", "Milk Standard 3L", "Woolworths", 7.19)],
        1,
    )
    .await;

    f.cmd()
        .args(["search", "milk"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Milk Standard 3L"));
}

#[tokio::test]
async fn an_overridden_store_is_not_headed_with_the_saved_stores_name() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_search(
        vec![product("282768", "Milk Standard 3L", "Woolworths", 7.19)],
        1,
    )
    .await;

    f.cmd().args(["store", "set", "regent"]).assert().success();

    // The saved name describes the saved id only. Heading a listing priced at
    // 9195 with "Regent Woolworths" would put the wrong shop above the right
    // prices.
    f.cmd()
        .args(["--store", "9195", "search", "milk"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Woolworths — 9195"))
        .stdout(predicate::str::contains("Regent").not());
}

#[tokio::test]
async fn buy_again_needs_an_account() {
    let f = Fixture::start().await;
    f.mount_store_selection().await;
    f.mount_op_error(
        "ProductSearch",
        "The current user is not authorized to access this resource.",
        "AUTH_NOT_AUTHENTICATED",
    )
    .await;

    f.cmd_signed_in()
        .args(["orders", "previous"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("wwnz auth login"));
}
