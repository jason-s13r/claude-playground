//! What the binary does before it would touch a network: flag precedence, the
//! refusals, the exit codes and the shape of `--json`.
//!
//! Wire decoding is tested in `fsnz-api`, against its own mock server. Nothing
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
        .stdout(contains("fsnz").or(contains("0.")));
}

#[test]
fn a_command_with_no_banner_chosen_says_how_to_choose_one() {
    Sandbox::new()
        .cmd()
        .args(["search", "milk"])
        .assert()
        .code(2)
        .stderr(contains("-b nw").and(contains("-b pns")));
}

#[test]
fn a_banner_with_no_store_exits_five_and_names_the_fix() {
    // 5 rather than 1 so a script can offer to pick a store instead of
    // reporting a generic failure. The command in the hint comes from this
    // binary, not from gsnz-core, which only names a remedy.
    Sandbox::new()
        .cmd()
        .args(["-b", "nw", "search", "milk"])
        .assert()
        .code(5)
        .stderr(contains("run `fsnz -b nw store set"));
}

#[test]
fn woolworths_is_not_a_banner_this_tool_speaks() {
    Sandbox::new()
        .cmd()
        .args(["-b", "ww", "search", "milk"])
        .assert()
        .code(2)
        .stderr(contains("New World and PAK'nSAVE only"));
}

#[test]
fn an_unknown_banner_is_rejected_with_the_spellings_that_work() {
    Sandbox::new()
        .cmd()
        .args(["-b", "tesco", "search", "milk"])
        .assert()
        .failure()
        .stderr(contains("nw, pns"));
}

#[test]
fn the_configured_banner_is_used_when_no_flag_names_one() {
    let sandbox = Sandbox::new();
    sandbox.write_config("banner = \"paknsave\"\n");
    sandbox
        .cmd()
        .args(["search", "milk"])
        .assert()
        // Exit 5 (no store) rather than 2 (no banner) proves the config
        // picked PAK'nSAVE.
        .code(5);
}

#[test]
fn a_flag_beats_the_configured_banner() {
    let sandbox = Sandbox::new();
    sandbox.write_config("banner = \"paknsave\"\n[newworld]\nstore_id = \"s1\"\n");
    sandbox
        .cmd()
        .args(["-b", "nw", "search", "milk"])
        .assert()
        // New World has a store, so this reaches the (dead) network: exit 1,
        // not the exit 5 an unstored PAK'nSAVE would give.
        .code(1);
}

#[test]
fn a_config_typo_is_reported_instead_of_ignored() {
    let sandbox = Sandbox::new();
    sandbox.write_config("baner = \"pns\"\n");
    sandbox
        .cmd()
        .args(["store", "show"])
        .assert()
        .failure()
        .stderr(contains("baner"));
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
        .stdout(contains("New World"))
        .stdout(contains("fsnz -b pns store set"));
}

#[test]
fn store_show_json_is_a_shape_a_script_can_read() {
    let sandbox = Sandbox::new();
    sandbox.write_config("[paknsave]\nstore_id = \"9999\"\n");
    let out = sandbox
        .cmd()
        .args(["--json", "-b", "pns", "store", "show"])
        .assert()
        .success();
    let value: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("valid JSON");
    assert_eq!(value["stores"][0]["retailer"], "paknsave");
    assert_eq!(value["stores"][0]["store_id"], "9999");
}

#[test]
fn store_clear_forgets_only_the_banner_it_was_given() {
    let sandbox = Sandbox::new();
    sandbox.write_config("[newworld]\nstore_id = \"s1\"\n\n[paknsave]\nstore_id = \"9999\"\n");
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
    // `source <(fsnz completions zsh)` breaks the moment anything else is on
    // the stream.
    Sandbox::new()
        .cmd()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(contains("_fsnz"));
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
fn compare_with_one_banner_is_refused_rather_than_run() {
    // One column is not a comparison, and running it would look like it worked.
    Sandbox::new()
        .cmd()
        .args(["-b", "nw", "compare", "milk"])
        .assert()
        .code(2)
        .stderr(contains("both banners"));
}

#[test]
fn compare_spans_both_banners_named_as_a_list() {
    // Both banners want a store, so reaching those failures proves the list
    // parsed and both sides were built.
    Sandbox::new()
        .cmd()
        .args(["-b", "nw,pns", "compare", "milk"])
        .assert()
        .success()
        .stderr(contains("New World could not be included"))
        .stderr(contains("PAK'nSAVE could not be included"));
}

#[test]
fn a_banner_that_cannot_answer_does_not_take_the_other_down_with_it() {
    // The whole point of reporting per-banner failures on stderr: the table
    // still prints, and exit stays 0.
    Sandbox::new()
        .cmd()
        .args(["--json", "-b", "nw,pns", "compare", "milk"])
        .assert()
        .success()
        .stdout(contains("["));
}

#[test]
fn more_than_one_banner_is_refused_for_a_command_that_needs_exactly_one() {
    Sandbox::new()
        .cmd()
        .args(["-b", "nw,pns", "cart", "list"])
        .assert()
        .code(2)
        .stderr(contains("only `compare` can span more than one"));
}

#[test]
fn emptying_the_cart_takes_more_than_asking() {
    Sandbox::new()
        .cmd()
        .args(["-b", "nw", "cart", "clear"])
        .assert()
        .code(2)
        .stderr(contains("--force"));
}

#[test]
fn auth_status_answers_for_both_banners_when_none_is_named() {
    Sandbox::new()
        .cmd()
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(contains("New World"))
        .stdout(contains("PAK'nSAVE"))
        .stdout(contains("signed out"));
}

#[test]
fn auth_status_json_is_an_array_a_script_can_index() {
    let out = Sandbox::new()
        .cmd()
        .args(["--json", "auth", "status", "-b", "pns"])
        .assert()
        .success();
    let value: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("valid JSON");
    assert_eq!(value[0]["retailer"], "paknsave");
    assert_eq!(value[0]["signed_in"], false);
}

#[test]
fn importing_a_file_that_is_not_there_says_what_the_file_should_be() {
    Sandbox::new()
        .cmd()
        .args(["-b", "nw", "auth", "import", "/nonexistent/cookies.txt"])
        .assert()
        .code(2)
        .stderr(contains("cookies.txt"));
}

#[test]
fn logging_out_when_never_signed_in_is_not_an_error() {
    Sandbox::new()
        .cmd()
        .args(["-b", "nw", "auth", "logout"])
        .assert()
        .success()
        .stdout(contains("Was not signed in"));
}

#[test]
fn doctor_leads_with_the_header_then_a_section_per_banner() {
    let sandbox = Sandbox::new();
    sandbox.write_config("banner = \"nw\"\n");
    // Nothing listens on the sandbox's hosts, so every banner is unreachable
    // -- which is the point: the layout has to hold when the probes fail.
    let out = sandbox.cmd().arg("doctor").assert().code(1);
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();

    assert!(text.starts_with("fsnz "), "{text}");
    for label in ["config file", "state dir", "default", "secrets"] {
        assert!(text.contains(label), "no {label} in:\n{text}");
    }
    for banner in ["New World", "PAK'nSAVE"] {
        assert!(text.contains(banner), "no {banner} in:\n{text}");
    }
    assert!(!text.contains("Woolworths"), "{text}");
    // Sections are indented under their heading; the header block is not.
    assert!(text.contains("\n  storefront"), "{text}");
    assert!(!text.contains('\u{250c}'), "no box drawing: {text}");
}

#[test]
fn doctor_stays_quiet_about_capabilities_when_there_are_no_gaps() {
    // The matrix exists to surface gaps. Both banners can do everything, so it
    // is not printed at all.
    Sandbox::new()
        .cmd()
        .arg("doctor")
        .assert()
        .code(1)
        .stdout(contains("Not available everywhere").not())
        .stdout(contains("orders previous").not());
}

#[test]
fn a_config_file_takes_the_spellings_the_flag_takes() {
    // `-b nw` working while `banner = "nw"` failed reads as the tool being
    // broken, and the file is the half people write by hand.
    for spelling in ["nw", "new-world", "New World"] {
        let sandbox = Sandbox::new();
        sandbox.write_config(&format!("banner = \"{spelling}\"\n"));
        sandbox
            .cmd()
            .args(["store", "show"])
            .assert()
            .success()
            .stdout(contains("New World"));
    }
}

#[test]
fn the_long_version_names_every_library_it_was_built_against() {
    // These release on their own tags, so "fsnz 0.1.0" alone does not say
    // which fsnz-api is compiled in -- and that is the part that breaks when
    // Foodstuffs changes its API.
    let out = Sandbox::new().cmd().arg("--version").assert().success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    for lib in [
        "gsnz-core",
        "gsnz-ui",
        "cli-kit",
        "net-kit",
        "fsnz-api",
        "build-kit",
    ] {
        assert!(text.contains(lib), "no {lib} in:\n{text}");
    }
    assert!(!text.contains("wwnz-api"), "no Woolworths API: {text}");
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
fn signing_out_of_one_banner_names_the_other_it_also_signs_out() {
    // One Club Plus account covers both, so this is not a surprise to
    // discover later.
    Sandbox::new()
        .cmd()
        .args(["-b", "nw", "auth", "logout"])
        .assert()
        .success()
        .stdout(contains("New World and PAK'nSAVE"));
}

#[test]
fn auth_without_a_banner_walks_the_one_credential_once() {
    // Two banners, one account. Asking for the Club Plus password twice is the
    // thing this avoids.
    let out = Sandbox::new()
        .cmd()
        .args(["auth", "logout"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("New World and PAK'nSAVE"), "{text}");
}

#[test]
fn use_sets_the_default_banner_and_reads_it_back() {
    let sandbox = Sandbox::new();
    sandbox
        .cmd()
        .args(["use", "pns"])
        .assert()
        .success()
        .stdout(contains("banner = paknsave"));
    sandbox
        .cmd()
        .arg("use")
        .assert()
        .success()
        .stdout(contains("paknsave"));
    // And it takes effect: a bare `search` reaches the PAK'nSAVE no-store
    // failure rather than the no-banner one.
    sandbox.cmd().args(["search", "milk"]).assert().code(5);
}

#[test]
fn use_can_be_changed_again() {
    let sandbox = Sandbox::new();
    sandbox.write_config("banner = \"nw\"\n");
    sandbox.cmd().args(["use", "pns"]).assert().success();
    assert!(sandbox.read_config().contains("paknsave"));
}

#[test]
fn the_config_file_keeps_only_what_was_changed() {
    // It is still a file people edit by hand, and one listing every default
    // cannot be skimmed.
    let sandbox = Sandbox::new();
    sandbox.cmd().args(["use", "pns"]).assert().success();
    let text = sandbox.read_config();
    assert!(text.contains("paknsave"), "{text}");
    assert!(!text.contains("[output]"), "{text}");
    assert!(!text.contains("[newworld]"), "{text}");
}

#[test]
fn config_get_prints_the_value_alone_so_it_can_be_captured() {
    let sandbox = Sandbox::new();
    sandbox.write_config("banner = \"pns\"\n");
    let out = sandbox
        .cmd()
        .args(["config", "get", "banner"])
        .assert()
        .success();
    assert_eq!(
        String::from_utf8_lossy(&out.get_output().stdout).trim(),
        "paknsave"
    );
}

#[test]
fn an_unset_value_leaves_stdout_empty_rather_than_saying_none() {
    // `$(fsnz config get auth.password_command)` must be the empty string.
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
        .args(["config", "set", "banner", "tesco"])
        .assert()
        .code(2)
        .stderr(contains("nw, pns"));
    assert!(!sandbox.read_config().contains("tesco"));
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
        "banner",
        "compare.retailers",
        "compare.match",
        "auth.password_command",
        "auth.store_password",
        "output.color",
        "newworld.store_id",
        "paknsave.token_command",
    ] {
        assert!(text.contains(key), "no {key} in:\n{text}");
    }
    assert!(!text.contains("woolworths"), "{text}");
}

#[test]
fn a_global_token_flag_is_not_a_thing() {
    // The combined tool had a global --token that only Foodstuffs read; here
    // a token is a per-banner config value, not a flag.
    Sandbox::new()
        .cmd()
        .args(["--token", "abc", "search", "milk"])
        .assert()
        .failure()
        .stderr(contains("--token"));
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
fn store_is_taken_by_the_commands_that_do() {
    Sandbox::new()
        .cmd()
        .args(["-b", "nw", "search", "milk", "--store", "abc"])
        .assert()
        // Reached the network rather than a parse error, which is as far as
        // this can go without a server.
        .code(1);
}

#[test]
fn a_listing_footer_names_the_command_only_because_the_app_supplied_it() {
    let sandbox = Sandbox::new();
    sandbox.write_config("banner = \"nw\"\n[newworld]\nstore_id = \"s1\"\n");
    // No network in the sandbox, so this only has to reach the failure rather
    // than render rows; the footer text itself is covered in gsnz-ui.
    sandbox.cmd().args(["stores"]).assert().failure();
}
