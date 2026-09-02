//! End-to-end: argument handling and `fsnz doctor`.
//!
//! Nothing here touches the internet -- both banners' endpoints are pointed at
//! a local server, which is also what makes the request bodies assertable.

mod support;

use support::*;

#[tokio::test]
async fn doctor_is_green_when_everything_is_configured() {
    let f = Fixture::start().await;
    let out = f
        .cmd_with_stores()
        .args(["--json", "doctor"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );

    let json = stdout_json(&out);
    assert_eq!(json["healthy"], true);
    assert_eq!(json["banners"]["newworld"]["api_reachable"], true);
    assert_eq!(
        json["banners"]["newworld"]["store_name"],
        "New World Thorndon"
    );
    assert_eq!(json["banners"]["paknsave"]["stores_returned"], 1);
}

#[tokio::test]
async fn doctor_fails_when_no_store_is_selected() {
    let f = Fixture::start().await;
    let out = f.cmd().args(["--json", "doctor"]).output().unwrap();

    assert!(!out.status.success(), "doctor should gate scripts");
    let json = stdout_json(&out);
    assert_eq!(json["healthy"], false);
    assert!(json["banners"]["newworld"]["store_id"].is_null());
}

#[tokio::test]
async fn a_bare_invocation_prints_the_help_rather_than_a_usage_error() {
    let f = Fixture::start().await;
    let out = f.cmd().output().unwrap();

    // Someone typing `fsnz` wants the commands, so this is a success on stdout
    // and not clap's terse "requires a subcommand" on stderr.
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Usage: fsnz"), "got: {stdout}");
    assert!(
        stdout.contains("fsnz auth login --email"),
        "the long help: {stdout}"
    );
    assert!(stdout.contains("compare"), "got: {stdout}");
}

#[tokio::test]
async fn an_unknown_subcommand_is_still_an_error() {
    let f = Fixture::start().await;
    let out = f.cmd().arg("bogus").output().unwrap();

    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unrecognized subcommand"));
}

#[tokio::test]
async fn an_unknown_banner_is_rejected_before_any_request() {
    let f = Fixture::start().await;
    let out = f
        .cmd()
        .args(["--banner", "countdown", "stores"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown banner"));
}

#[tokio::test]
async fn completions_emit_a_script_for_the_named_shell() {
    let f = Fixture::start().await;
    let out = f.cmd().args(["completions", "zsh"]).output().unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Only the script goes to stdout, so `source <(fsnz completions zsh)` works.
    assert!(
        out.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("#compdef fsnz"), "got: {stdout}");
    assert!(stdout.contains("'compare:"), "the subcommands: {stdout}");
}

#[tokio::test]
async fn completions_fall_back_to_the_shell_in_the_environment() {
    let f = Fixture::start().await;
    let out = f
        .cmd()
        .arg("completions")
        .env("SHELL", "/usr/bin/fish")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("__fish_fsnz_global_optspecs"),
        "got: {stdout}"
    );
}

#[tokio::test]
async fn completions_say_which_shells_they_know_when_the_environment_does_not() {
    let f = Fixture::start().await;
    let out = f
        .cmd()
        .arg("completions")
        .env("SHELL", "/usr/bin/nonsuch")
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("bash|zsh|fish"), "got: {stderr}");
}

#[tokio::test]
async fn completions_do_not_need_a_working_config() {
    let f = Fixture::start().await;
    let config_dir = f.home.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.toml"), "this is not toml {{{").unwrap();

    // Every other command would fail on that file. Generating a script depends
    // on nothing but the command surface, so it must not read it.
    let out = f.cmd().args(["completions", "bash"]).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let broken = f.cmd().args(["stores"]).output().unwrap();
    assert!(!broken.status.success(), "the config really is broken");
}
