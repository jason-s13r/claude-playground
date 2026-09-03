//! What the binary does before it would touch a network: flag precedence, the
//! refusals, the exit codes and the shape of `--json`.
//!
//! Wire decoding is tested in the api crates, against their own mock servers.
//! Nothing here needs one, which is why these run in milliseconds.

mod support;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use support::Sandbox;

#[test]
fn the_version_carries_a_commit_and_a_date() {
    Sandbox::new()
        .cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("gsnz"));
}

#[test]
fn a_command_with_no_shop_chosen_says_how_to_choose_one() {
    Sandbox::new()
        .cmd()
        .args(["search", "milk"])
        .assert()
        .code(2)
        .stderr(contains("-b nw").and(contains("-b ww")));
}

#[test]
fn a_shop_with_no_store_exits_five_and_names_the_fix() {
    // 5 rather than 1 so a script can offer to pick a store instead of
    // reporting a generic failure.
    Sandbox::new()
        .cmd()
        .args(["-b", "nw", "search", "milk"])
        .assert()
        .code(5)
        .stderr(contains("store set"));
}

#[test]
fn woolworths_refuses_a_per_command_store_rather_than_quoting_another_ones_prices() {
    // 4 is "this shop cannot do that". Silently searching the cart's store
    // while `--store` named a different one would be a wrong-price bug.
    Sandbox::new()
        .cmd()
        .args(["-b", "ww", "--store", "9999", "search", "milk"])
        .assert()
        .code(4)
        .stderr(contains("store set"));
}

#[test]
fn an_unknown_shop_is_rejected_with_the_spellings_that_work() {
    Sandbox::new()
        .cmd()
        .args(["-b", "tesco", "search", "milk"])
        .assert()
        .failure()
        .stderr(contains("nw, pns or ww"));
}

#[test]
fn the_configured_shop_is_used_when_no_flag_names_one() {
    let sandbox = Sandbox::new();
    sandbox.write_config("retailer = \"woolworths\"\n");
    sandbox
        .cmd()
        .args(["--store", "9999", "search", "milk"])
        .assert()
        // Reaching the Woolworths refusal proves the config picked the shop.
        .code(4);
}

#[test]
fn a_flag_beats_the_configured_shop() {
    let sandbox = Sandbox::new();
    sandbox.write_config("retailer = \"woolworths\"\n");
    sandbox
        .cmd()
        .args(["-b", "nw", "search", "milk"])
        .assert()
        .code(5);
}

#[test]
fn a_config_typo_is_reported_instead_of_ignored() {
    let sandbox = Sandbox::new();
    sandbox.write_config("retialer = \"ww\"\n");
    sandbox
        .cmd()
        .args(["store", "show"])
        .assert()
        .failure()
        .stderr(contains("retialer"));
}

#[test]
fn store_show_answers_without_a_network_call() {
    // What someone runs when a command has just failed, so it must not depend
    // on the thing that failed.
    let sandbox = Sandbox::new();
    sandbox.write_config("[newworld]\nstore_id = \"s1\"\n");
    sandbox
        .cmd()
        .args(["store", "show"])
        .assert()
        .success()
        .stdout(contains("New World  s1"))
        .stdout(contains("gsnz -b ww store set"));
}

#[test]
fn store_show_json_is_a_shape_a_script_can_read() {
    let sandbox = Sandbox::new();
    sandbox.write_config("[woolworths]\nstore_id = \"9999\"\n");
    let out = sandbox
        .cmd()
        .args(["--json", "-b", "ww", "store", "show"])
        .assert()
        .success();
    let value: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("valid JSON");
    assert_eq!(value["stores"][0]["retailer"], "woolworths");
    assert_eq!(value["stores"][0]["store_id"], "9999");
}

#[test]
fn store_clear_forgets_only_the_shop_it_was_given() {
    let sandbox = Sandbox::new();
    sandbox.write_config("[newworld]\nstore_id = \"s1\"\n\n[woolworths]\nstore_id = \"9999\"\n");
    sandbox
        .cmd()
        .args(["-b", "nw", "store", "clear"])
        .assert()
        .success()
        .stdout(contains("forgot store s1"));
    let config = sandbox.read_config();
    assert!(!config.contains("s1"), "{config}");
    assert!(config.contains("9999"), "{config}");
}

#[test]
fn completions_write_a_script_and_nothing_else() {
    // `source <(gsnz completions zsh)` breaks the moment anything else is on
    // the stream.
    Sandbox::new()
        .cmd()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(contains("_gsnz"));
}

#[test]
fn completions_name_the_shells_on_offer_when_the_shell_is_unknown() {
    Sandbox::new()
        .cmd()
        .env_remove("SHELL")
        .arg("completions")
        .assert()
        .failure()
        .stderr(contains("bash|zsh|fish"));
}
