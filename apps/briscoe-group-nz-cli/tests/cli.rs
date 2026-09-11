//! The binary, run as a binary.
//!
//! Aimed at what unit tests cannot reach: that `-b` actually reaches the
//! catalogue, that a signed-out command fails in the way a script can act on,
//! and that the two fascias' credentials stay apart on disk.

use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

/// A run with its own config and state, so tests never touch a real one.
fn bgnz(home: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("bgnz").expect("the binary builds");
    cmd.env("BGNZ_CONFIG_DIR", home.path().join("config"))
        .env("BGNZ_STATE_DIR", home.path().join("state"))
        // Never the system keychain: a test must not prompt, and must not
        // leave anything behind.
        .env("BGNZ_SECRET_BACKEND", "file")
        .env("NO_COLOR", "1");
    cmd
}

#[test]
fn the_help_names_both_fascias() {
    let home = tempfile::tempdir().expect("a temp dir");
    bgnz(&home)
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Briscoes").and(contains("Rebel Sport")));
}

#[test]
fn an_unknown_fascia_is_refused_before_anything_is_fetched() {
    let home = tempfile::tempdir().expect("a temp dir");
    bgnz(&home)
        .args(["-b", "kmart", "search", "jug"])
        .assert()
        .code(2)
        .stderr(contains("is not a fascia"));
}

#[test]
fn a_signed_out_command_exits_three_and_says_which_fascia() {
    // Exit 3 is the whole point: a script driving this can tell "sign in"
    // from "that product does not exist" without reading the message.
    let home = tempfile::tempdir().expect("a temp dir");
    bgnz(&home)
        .args(["-b", "rebel", "cart", "list"])
        .assert()
        .code(3)
        .stderr(contains("Rebel Sport").and(contains("auth login")));
}

#[test]
fn auth_status_reports_both_fascias_even_when_neither_is_signed_in() {
    // Being signed in to one and not the other is ordinary here, so showing
    // only the current fascia is how someone concludes the tool forgot them.
    let home = tempfile::tempdir().expect("a temp dir");
    bgnz(&home)
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(contains("Briscoes").and(contains("Rebel Sport")));
}

#[test]
fn each_fascia_keeps_its_own_store_setting() {
    let home = tempfile::tempdir().expect("a temp dir");
    bgnz(&home)
        .args(["config", "set", "store.briscoes", "291"])
        .assert()
        .success();
    bgnz(&home)
        .args(["config", "get", "store.rebelsport"])
        .assert()
        .success()
        .stdout(contains("291").not());
}

#[test]
fn the_state_directories_are_separate_per_fascia() {
    // No SSO between them, so `logout` for one must visibly leave the other
    // alone -- which starts with them not sharing a file.
    let home = tempfile::tempdir().expect("a temp dir");
    let out = bgnz(&home).args(["config", "path"]).output().expect("runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("briscoes"), "{text}");
    assert!(text.contains("rebelsport"), "{text}");
}

#[test]
fn stock_without_a_store_says_how_to_choose_one() {
    let home = tempfile::tempdir().expect("a temp dir");
    bgnz(&home)
        .args(["stock", "1103832"])
        .assert()
        .code(2)
        .stderr(contains("bgnz store set"));
}

#[test]
fn completions_write_a_script_and_nothing_else() {
    // `source <(bgnz completions zsh)` has to work, which means no banner and
    // no progress on stdout.
    let home = tempfile::tempdir().expect("a temp dir");
    let out = bgnz(&home)
        .args(["completions", "zsh"])
        .output()
        .expect("runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("#compdef bgnz"),
        "{}",
        &text[..80.min(text.len())]
    );
}
