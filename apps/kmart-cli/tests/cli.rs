//! The binary, end to end, with no network.
//!
//! Everything here runs against a config and state directory of its own, so a
//! test never reads or writes the machine's real ones. The commands that would
//! talk to Kmart are not exercised here -- `kmart-api` owns those against a
//! mock server -- so what is left is the part this crate is responsible for:
//! flags, config, country resolution, and the exit code a script sees.

use assert_cmd::Command;
use predicates::str::contains;

/// A run with its own config and state, and no colour.
fn kmart(home: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("kmart").expect("the binary");
    cmd.env("KMART_CONFIG_DIR", home.path())
        .env("KMART_STATE_DIR", home.path())
        .env("KMART_SECRET_BACKEND", "file")
        .env("NO_COLOR", "1")
        // Pointed at a port nothing listens on, so a command that would reach
        // Kmart fails as a connection error rather than by contacting it.
        .env("KMART_API", "http://127.0.0.1:9")
        .env("KMART_SEARCH", "http://127.0.0.1:9")
        .env("KMART_AUTH", "http://127.0.0.1:9");
    cmd
}

fn home() -> tempfile::TempDir {
    tempfile::TempDir::new().expect("a temp dir")
}

#[test]
fn version_names_the_libraries_it_was_built_from() {
    // The point of printing them: "kmart 0.0.0" does not say which kmart-api
    // was compiled in, and that is the part that breaks when Kmart changes an
    // endpoint.
    let home = home();
    kmart(&home)
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("kmart-api"))
        .stdout(contains("cli-kit"));
}

#[test]
fn help_lists_the_commands() {
    let home = home();
    kmart(&home)
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("search"))
        .stdout(contains("stock"))
        .stdout(contains("wishlist"))
        .stdout(contains("use"));
}

#[test]
fn the_default_country_is_australia_and_use_changes_it() {
    let home = home();
    kmart(&home)
        .arg("use")
        .assert()
        .success()
        .stdout(contains("Australia"));

    kmart(&home)
        .args(["use", "nz"])
        .assert()
        .success()
        .stdout(contains("New Zealand"));

    // And it sticks, which is the whole point of the command.
    kmart(&home)
        .arg("use")
        .assert()
        .success()
        .stdout(contains("New Zealand"));
}

#[test]
fn the_country_flag_beats_the_saved_one_without_changing_it() {
    let home = home();
    kmart(&home).args(["use", "nz"]).assert().success();
    kmart(&home)
        .args(["config", "get", "country"])
        .assert()
        .success()
        .stdout(contains("nz"));

    // A flag is for one command, so the file must be untouched afterwards.
    // `auth login` is the documented exception, and it is not this.
    kmart(&home)
        .args(["--country", "au", "config", "get", "country"])
        .assert()
        .success()
        .stdout(contains("nz"));
}

#[test]
fn an_unknown_country_is_refused_before_anything_is_attempted() {
    let home = home();
    kmart(&home)
        .args(["--country", "uk", "search", "mop"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn stock_without_a_postcode_says_how_to_set_one() {
    // There is no sensible default -- guessing a city would quote stock for
    // somewhere the person is not -- so this is an error with a way out.
    let home = home();
    kmart(&home)
        .args(["stock", "43165537"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("kmart postcode set"));
}

#[test]
fn an_account_command_with_no_session_exits_three_and_says_to_sign_in() {
    let home = home();
    kmart(&home)
        .arg("orders")
        .assert()
        .failure()
        // Not signed in and not admitted: the challenge is reported first,
        // because importing cookies is what has to happen before a token is
        // of any use.
        .code(8)
        .stderr(contains("auth import"));
}

#[test]
fn an_account_command_reports_the_missing_sign_in_once_cookies_are_present() {
    let home = home();
    let cookies = home.path().join("cookies.txt");
    // Australian, because that is the country a fresh install asks: cookies for
    // the other one are not admission here, and the test would be measuring
    // that instead.
    std::fs::write(
        &cookies,
        ".kmart.com.au\tTRUE\t/\tTRUE\t9999999999\t_abck\tadmission\n",
    )
    .unwrap();
    kmart(&home)
        .args(["auth", "import", cookies.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("au"));

    kmart(&home)
        .arg("orders")
        .assert()
        .failure()
        .code(3)
        // Cookies are admission, not a sign-in. The advice names the command
        // that gets an account token -- `auth login`, which drives a browser,
        // rather than either of the paste-it-in-yourself ways round it.
        .stderr(contains("auth login"));
}

#[test]
fn importing_a_file_with_no_kmart_cookies_says_what_to_do() {
    let home = home();
    let cookies = home.path().join("other.txt");
    std::fs::write(
        &cookies,
        ".example.test\tTRUE\t/\tTRUE\t9999999999\t_abck\tsomeone-else\n",
    )
    .unwrap();
    kmart(&home)
        .args(["auth", "import", cookies.to_str().unwrap()])
        .assert()
        .failure()
        .code(8)
        .stderr(contains("Sign in at kmart"));
}

#[test]
fn auth_status_reports_the_two_credentials_separately() {
    let home = home();
    kmart(&home)
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(contains("Signed out"))
        .stdout(contains("No bot-check cookies"));
}

#[test]
fn refresh_with_nothing_to_renew_exits_three_rather_than_succeeding_quietly() {
    // The whole point of the command is running unattended, so the one thing
    // it must never do is report success having renewed nothing.
    let home = home();
    kmart(&home)
        .args(["auth", "refresh"])
        .assert()
        .failure()
        .code(3)
        .stderr(contains("auth login"));
}

#[test]
fn refresh_needs_more_than_cookies() {
    // Admission is not a sign-in, and there is no password to become one.
    let home = home();
    let cookies = home.path().join("cookies.txt");
    std::fs::write(
        &cookies,
        ".kmart.com.au\tTRUE\t/\tTRUE\t9999999999\t_abck\tadmission\n",
    )
    .unwrap();
    kmart(&home)
        .args(["auth", "import", cookies.to_str().unwrap()])
        .assert()
        .success();

    kmart(&home)
        .args(["auth", "refresh"])
        .assert()
        .failure()
        .code(3)
        .stderr(contains("auth login"));
}

#[test]
fn refresh_will_not_take_both_a_window_and_no_browser_at_all() {
    let home = home();
    kmart(&home)
        .args(["auth", "refresh", "--headful", "--direct"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn config_list_shows_every_setting_and_what_it_does() {
    let home = home();
    kmart(&home)
        .args(["config", "list"])
        .assert()
        .success()
        .stdout(contains("country"))
        .stdout(contains("postcode"))
        .stdout(contains("not set"));
}

#[test]
fn a_bad_config_value_is_refused_at_the_point_it_is_set() {
    let home = home();
    kmart(&home)
        .args(["config", "set", "output.color", "purple"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("auto, always or never"));
}

#[test]
fn config_get_prints_the_value_and_nothing_else() {
    let home = home();
    kmart(&home).args(["use", "nz"]).assert().success();
    let out = kmart(&home)
        .args(["config", "get", "country"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "nz\n");
}

#[test]
fn the_island_setting_normalises_what_was_typed() {
    let home = home();
    kmart(&home)
        .args(["island", "set", "south"])
        .assert()
        .success()
        .stdout(contains("SI"));
    kmart(&home)
        .arg("island")
        .assert()
        .success()
        .stdout(contains("SI"));
    kmart(&home)
        .args(["island", "set", "middle"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn the_island_setting_says_it_does_nothing_in_australia() {
    let home = home();
    kmart(&home)
        .args(["--country", "au", "island"])
        .assert()
        .success()
        .stdout(contains("only affects New Zealand"));
}

#[test]
fn a_malformed_filter_is_refused_with_the_text_that_was_typed() {
    let home = home();
    kmart(&home)
        .args(["search", "mop", "--filter", "justaname"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("justaname"));
}

#[test]
fn completions_are_generated_for_a_named_shell() {
    let home = home();
    kmart(&home)
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(contains("#compdef kmart"));
}

#[test]
fn an_unknown_shell_is_refused_rather_than_guessed_at() {
    let home = home();
    kmart(&home)
        .args(["completions", "csh"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn config_path_says_where_the_file_is() {
    let home = home();
    kmart(&home)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(contains("config.toml"));
}

#[test]
fn doctor_fails_when_kmart_cannot_be_reached_and_says_which_half() {
    // Pointed at a dead port, so the catalogue call fails. The report should
    // still print, and the exit code carry, without a second error on top.
    let home = home();
    kmart(&home)
        .arg("doctor")
        .assert()
        .failure()
        .code(1)
        .stdout(contains("catalogue"))
        .stdout(contains("not healthy"));
}

#[test]
fn doctor_warns_about_a_missing_postcode_because_two_commands_need_one() {
    let home = home();
    kmart(&home)
        .arg("doctor")
        .assert()
        .failure()
        .stdout(contains("stock and stores will not run"));
}

#[test]
fn json_is_a_document_on_stdout() {
    let home = home();
    let out = kmart(&home)
        .args(["--json", "auth", "status"])
        .output()
        .unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout is one JSON document");
    assert_eq!(value["signed_in"], false);
    assert!(value["admitted"].as_array().unwrap().is_empty());
}
