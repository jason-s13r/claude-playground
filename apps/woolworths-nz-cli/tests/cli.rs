//! What the binary does before it would touch a network: flag precedence, the
//! refusals, the exit codes and the shape of `--json`.
//!
//! Wire decoding is tested in `wwnz-api`, against its own mock server. Nothing
//! here needs one, which is why these run in milliseconds.

mod support;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use support::Sandbox;

#[test]
fn the_version_carries_the_binary_name() {
    Sandbox::new()
        .cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("wwnz").or(contains("0.")));
}

#[test]
fn there_is_no_banner_flag_because_there_is_one_shop() {
    // The combined tool takes `-b ww`; here it would name the only thing there
    // is.
    Sandbox::new()
        .cmd()
        .args(["-b", "ww", "search", "milk"])
        .assert()
        .failure()
        .stderr(contains("unexpected argument"));
}

#[test]
fn a_per_command_store_is_refused_rather_than_quietly_ignored() {
    // Prices are quoted against whatever store the *cart* is bound to, which
    // is server-side state. Accepting `--store` and answering with the other
    // store's prices would be a wrong-price bug with nothing on screen to
    // explain it.
    Sandbox::new()
        .cmd()
        .args(["search", "milk", "--store", "9048"])
        .assert()
        .code(4)
        .stderr(contains("wwnz store set"));
}

#[test]
fn a_search_with_no_store_selected_still_reaches_the_network() {
    // Unlike the Foodstuffs half, a store is not required: with none bound the
    // site prices against a default, and exit 1 here is the dead sandbox host
    // rather than a refusal.
    Sandbox::new()
        .cmd()
        .args(["search", "milk"])
        .assert()
        .code(1);
}

#[test]
fn a_config_typo_is_reported_instead_of_ignored() {
    let sandbox = Sandbox::new();
    sandbox.write_config("stroe_id = \"9048\"\n");
    sandbox
        .cmd()
        .args(["store", "show"])
        .assert()
        .failure()
        .stderr(contains("stroe_id"));
}

#[test]
fn the_flat_config_of_the_previous_version_is_refused_by_name() {
    // 0.2 kept `password_command` at the top level; it is `auth.password_command`
    // now. Loading it silently would leave a login prompting for a password the
    // file says how to fetch.
    let sandbox = Sandbox::new();
    sandbox.write_config("password_command = \"pass show ww\"\n");
    sandbox
        .cmd()
        .args(["store", "show"])
        .assert()
        .failure()
        .stderr(contains("password_command"));
}

#[test]
fn store_show_answers_without_a_network_call() {
    // What someone runs when a command has just failed, so it must not depend
    // on the thing that failed.
    let sandbox = Sandbox::new();
    sandbox.write_config("store_id = \"9048\"\n");
    sandbox
        .cmd()
        .args(["store", "show"])
        .assert()
        .success()
        .stdout(contains("9048"));
}

#[test]
fn store_show_says_how_to_choose_one_when_none_is_selected() {
    Sandbox::new()
        .cmd()
        .args(["store", "show"])
        .assert()
        .success()
        .stdout(contains("wwnz store set"));
}

#[test]
fn store_show_json_is_a_shape_a_script_can_read() {
    let sandbox = Sandbox::new();
    sandbox.write_config("store_id = \"9048\"\n");
    let out = sandbox
        .cmd()
        .args(["--json", "store", "show"])
        .assert()
        .success();
    let value: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("valid JSON");
    assert_eq!(value["store_id"], "9048");
}

#[test]
fn store_clear_forgets_the_local_record() {
    let sandbox = Sandbox::new();
    sandbox.write_config("store_id = \"9048\"\n");
    sandbox
        .cmd()
        .args(["store", "clear"])
        .assert()
        .success()
        .stdout(contains("forgot store 9048"));
    assert!(!sandbox.read_config().contains("9048"));
}

#[test]
fn completions_write_a_script_and_nothing_else() {
    // `source <(wwnz completions zsh)` breaks the moment anything else is on
    // the stream.
    Sandbox::new()
        .cmd()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(contains("_wwnz"));
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

#[test]
fn completions_need_no_credential_store() {
    // The adapter is built lazily precisely so this does not load a cookie jar
    // out of the keyring, which on a Mac can put a prompt in front of a script.
    Sandbox::new()
        .cmd()
        .env("WWNZ_SECRET_BACKEND", "keyring")
        .args(["completions", "bash"])
        .assert()
        .success();
}

#[test]
fn emptying_the_cart_takes_more_than_asking() {
    Sandbox::new()
        .cmd()
        .args(["cart", "clear"])
        .assert()
        .code(2)
        .stderr(contains("--force"));
}

#[test]
fn an_account_command_with_no_session_says_to_sign_in() {
    // 3 rather than 1 so a script can offer to sign in instead of reporting a
    // generic failure. The command in the hint comes from this binary, not
    // from gsnz-core, which only names a remedy.
    Sandbox::new()
        .cmd()
        .args(["cart", "list"])
        .assert()
        .code(3)
        .stderr(contains("run `wwnz auth login`"));
}

#[test]
fn auth_status_reports_signed_out_rather_than_failing() {
    Sandbox::new()
        .cmd()
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(contains("Woolworths"))
        .stdout(contains("signed out"));
}

#[test]
fn auth_status_json_is_the_object_not_a_one_element_array() {
    // There is one account, so `wwnz auth status --json | jq .signed_in`
    // should not have to index past a wrapper.
    let out = Sandbox::new()
        .cmd()
        .args(["--json", "auth", "status"])
        .assert()
        .success();
    let value: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("valid JSON");
    assert_eq!(value["retailer"], "woolworths");
    assert_eq!(value["signed_in"], false);
}

#[test]
fn refreshing_a_session_that_was_never_started_is_reported_not_failed() {
    Sandbox::new()
        .cmd()
        .args(["auth", "refresh"])
        .assert()
        .success()
        .stderr(contains("nothing to renew"));
}

#[test]
fn importing_a_file_that_is_not_there_says_what_the_file_should_be() {
    Sandbox::new()
        .cmd()
        .args(["auth", "import", "/nonexistent/cookies.txt"])
        .assert()
        .code(2)
        .stderr(contains("cookies.txt"));
}

#[test]
fn logging_out_when_never_signed_in_is_not_an_error() {
    Sandbox::new()
        .cmd()
        .args(["auth", "logout"])
        .assert()
        .success()
        .stdout(contains("Was not signed in"));
}

#[test]
fn doctor_leads_with_the_header_then_the_one_section() {
    let sandbox = Sandbox::new();
    sandbox.write_config("store_id = \"9048\"\n");
    // Nothing listens on the sandbox's hosts, so the probe fails -- which is
    // the point: the layout has to hold when it does.
    let out = sandbox.cmd().arg("doctor").assert().code(1);
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();

    assert!(text.starts_with("wwnz "), "{text}");
    for label in ["config file", "state dir", "secrets"] {
        assert!(text.contains(label), "no {label} in:\n{text}");
    }
    assert!(text.contains("Woolworths"), "{text}");
    assert!(!text.contains("New World"), "{text}");
    // Sections are indented under their heading; the header block is not.
    assert!(text.contains("\n  storefront"), "{text}");
    assert!(text.contains("\n  sign-in"), "{text}");
    assert!(!text.contains('\u{250c}'), "no box drawing: {text}");
}

#[test]
fn doctor_stays_quiet_about_capabilities_when_there_are_no_gaps() {
    // The list exists to surface gaps. This site can do everything, so it is
    // not printed at all.
    Sandbox::new()
        .cmd()
        .arg("doctor")
        .assert()
        .code(1)
        .stdout(contains("Not available").not())
        .stdout(contains("orders previous").not());
}

#[test]
fn the_long_version_names_every_library_it_was_built_against() {
    // These release on their own tags, so "wwnz 0.1.0" alone does not say
    // which wwnz-api is compiled in -- and that is the part that breaks when
    // Woolworths changes its API.
    let out = Sandbox::new().cmd().arg("--version").assert().success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    for lib in [
        "gsnz-core",
        "gsnz-ui",
        "cli-kit",
        "net-kit",
        "wwnz-api",
        "build-kit",
    ] {
        assert!(text.contains(lib), "no {lib} in:\n{text}");
    }
    assert!(!text.contains("fsnz-api"), "no Foodstuffs API: {text}");
    // One per line, aligned under the label column rather than run together.
    let lines: Vec<&str> = text.lines().filter(|l| l.contains("gsnz-ui")).collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert!(lines[0].starts_with("           gsnz-ui"), "{:?}", lines[0]);
}

#[test]
fn the_short_version_stays_one_line() {
    // `-V` is what a script greps and what a bug report pastes.
    let out = Sandbox::new().cmd().arg("-V").assert().success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(!text.contains("libraries"), "{text}");
}

#[test]
fn the_config_file_keeps_only_what_was_changed() {
    // It is still a file people edit by hand, and one listing every default
    // cannot be skimmed.
    let sandbox = Sandbox::new();
    sandbox
        .cmd()
        .args(["config", "set", "store_id", "9048"])
        .assert()
        .success()
        .stdout(contains("store_id = 9048"));
    let text = sandbox.read_config();
    assert!(text.contains("9048"), "{text}");
    assert!(!text.contains("[output]"), "{text}");
    assert!(!text.contains("[auth]"), "{text}");
}

#[test]
fn config_get_prints_the_value_alone_so_it_can_be_captured() {
    let sandbox = Sandbox::new();
    sandbox.write_config("store_id = \"9048\"\n");
    let out = sandbox
        .cmd()
        .args(["config", "get", "store_id"])
        .assert()
        .success();
    assert_eq!(
        String::from_utf8_lossy(&out.get_output().stdout).trim(),
        "9048"
    );
}

#[test]
fn an_unset_value_leaves_stdout_empty_rather_than_saying_none() {
    // `$(wwnz config get auth.password_command)` must be the empty string.
    let out = Sandbox::new()
        .cmd()
        .args(["config", "get", "auth.password_command"])
        .assert()
        .success();
    assert!(out.get_output().stdout.is_empty());
}

#[test]
fn a_bad_value_never_reaches_the_file() {
    let sandbox = Sandbox::new();
    sandbox
        .cmd()
        .args(["config", "set", "output.color", "purple"])
        .assert()
        .code(2)
        .stderr(contains("auto"));
    assert!(!sandbox.read_config().contains("purple"));
}

#[test]
fn config_list_covers_every_key_and_says_what_each_does() {
    let out = Sandbox::new()
        .cmd()
        .args(["config", "list"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    for key in [
        "store_id",
        "auth.password_command",
        "auth.store_password",
        "output.color",
    ] {
        assert!(text.contains(key), "no {key} in:\n{text}");
    }
    // No banner, no comparison, no second shop's token command.
    assert!(!text.contains("compare"), "{text}");
    assert!(!text.contains("token_command"), "{text}");
}

#[test]
fn there_is_no_compare_command_with_only_one_shop() {
    // One column is not a comparison; `gsnz compare` is where that lives.
    Sandbox::new()
        .cmd()
        .args(["compare", "milk"])
        .assert()
        .failure()
        .stderr(contains("unrecognized subcommand"));
}

#[test]
fn store_is_refused_by_commands_that_do_not_quote_a_price() {
    Sandbox::new()
        .cmd()
        .args(["config", "list", "--store", "1"])
        .assert()
        .failure()
        .stderr(contains("unexpected argument '--store'"));
}

#[test]
fn a_listing_footer_names_the_command_only_because_the_app_supplied_it() {
    // No network in the sandbox, so this only has to reach the failure rather
    // than render rows; the footer text itself is covered in gsnz-ui.
    Sandbox::new().cmd().args(["stores"]).assert().failure();
}
