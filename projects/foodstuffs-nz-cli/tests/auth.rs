//! End-to-end: Club Plus login, session renewal, status and logout.
//!
//! Nothing here touches the internet -- both banners' endpoints are pointed at
//! a local server, which is also what makes the request bodies assertable.

mod support;

use serde_json::json;
use support::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---- Club Plus login -----------------------------------------------------

/// Point the tool's Club Plus endpoints at a mock and log in.
fn with_clubplus<'a>(cmd: &'a mut assert_cmd::Command, base: &str) -> &'a mut assert_cmd::Command {
    cmd.env("FSNZ_CLUBPLUS_LOGIN", base)
        .env("FSNZ_CLUBPLUS_API", base)
}

#[tokio::test]
async fn login_stores_a_session_and_proves_it_mints_tokens() {
    let f = Fixture::start().await;
    let cp = MockServer::start().await;
    f.mount_login(&cp).await;

    let mut cmd = f.cmd_with_stores();
    let out = with_clubplus(&mut cmd, &cp.uri())
        .args([
            "--json",
            "auth",
            "login",
            "--email",
            "shopper@example.test",
            "--password-command",
            "echo hunter2",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = stdout_json(&out);
    assert_eq!(json["email"], "shopper@example.test");
    // Both banners are checked, because one login covers both.
    assert_eq!(json["banners"][0]["ok"], true);
    assert_eq!(json["banners"][1]["ok"], true);
}

/// The session Club Plus issues lasts about half an hour, the same as the
/// banner tokens minted from it. Without renewal a login would be good for one
/// sitting, which is the whole point of storing a refresh token.
#[tokio::test]
async fn an_expired_session_is_renewed_rather_than_forcing_another_login() {
    let f = Fixture::start().await;
    let cp = MockServer::start().await;
    f.mount_expired_login(&cp).await;

    let mut cmd = f.cmd_with_stores();
    let out = with_clubplus(&mut cmd, &cp.uri())
        .args([
            "auth",
            "login",
            "--email",
            "s@example.test",
            "--password-command",
            "echo pw",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // --refresh skips the cache `login` warmed, so the stored session is what
    // gets used -- and it is already expired.
    let mut mint = f.cmd_with_stores();
    let out = with_clubplus(&mut mint, &cp.uri())
        .args(["--json", "auth", "token", "--refresh"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "an expired session should renew itself: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let renewals = cp
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/user/login/refresh")
        .count();
    assert!(
        renewals >= 1,
        "the refresh endpoint should have been called"
    );

    // The rotated token has to be the one stored, or the next renewal fails.
    let mut status = f.cmd_with_stores();
    let out = with_clubplus(&mut status, &cp.uri())
        .args(["--json", "auth", "status"])
        .output()
        .unwrap();
    let json = stdout_json(&out);
    assert_eq!(json["session"]["fresh"], true, "renewal should have stuck");
    assert_eq!(json["session"]["can_renew"], true);
}

#[tokio::test]
async fn auth_status_reports_the_session_and_both_banners() {
    let f = Fixture::start().await;
    let cp = MockServer::start().await;
    f.mount_login(&cp).await;

    let mut cmd = f.cmd_with_stores();
    with_clubplus(&mut cmd, &cp.uri())
        .args([
            "auth",
            "login",
            "--email",
            "shopper@example.test",
            "--password-command",
            "echo pw",
        ])
        .output()
        .unwrap();

    let mut status = f.cmd_with_stores();
    let out = with_clubplus(&mut status, &cp.uri())
        .args(["--json", "auth", "status"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let json = stdout_json(&out);
    assert_eq!(json["logged_in"], true);
    assert_eq!(json["email"], "shopper@example.test");
    assert_eq!(json["session"]["banner_claim"], "NAT");
    // `login` proves both banners, so both have a token cached by now.
    assert_eq!(json["banners"]["newworld"]["banner_claim"], "MNW");
    assert_eq!(json["banners"]["paknsave"]["banner_claim"], "PNS");
    // The fixture account is linked to New World only, which is what decides
    // whether a banner's cart can work at all.
    assert_eq!(json["banners"]["newworld"]["linked"], true);
    assert_eq!(json["banners"]["paknsave"]["linked"], false);
}

#[tokio::test]
async fn auth_status_without_a_login_says_so_and_exits_non_zero() {
    let f = Fixture::start().await;
    let out = f.cmd().args(["--json", "auth", "status"]).output().unwrap();

    assert!(!out.status.success(), "status should gate scripts");
    let json = stdout_json(&out);
    assert_eq!(json["logged_in"], false);
    assert_eq!(json["usable"], false);
    assert!(json["email"].is_null());
}

#[tokio::test]
async fn a_stored_login_is_what_later_commands_mint_from() {
    let f = Fixture::start().await;
    let cp = MockServer::start().await;
    f.mount_login(&cp).await;
    mount_search(&f.newworld, search_response(vec![])).await;

    let mut cmd = f.cmd_with_stores();
    with_clubplus(&mut cmd, &cp.uri())
        .args([
            "auth",
            "login",
            "--email",
            "s@example.test",
            "--password-command",
            "echo pw",
        ])
        .output()
        .unwrap();

    // Minting from the stored session still goes through Club Plus, so the
    // later command needs those endpoints too. --refresh gets past the cache
    // `login` warmed, which is what makes this exercise the mint.
    let mut mint = f.cmd_with_stores();
    let out = with_clubplus(&mut mint, &cp.uri())
        .args(["--json", "auth", "token", "--refresh"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        stdout_json(&out)["source"],
        "minted from the stored Club Plus login"
    );

    // ...and the challenged storefront is never touched.
    let mints = f
        .newworld
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/")
        .count();
    assert_eq!(
        mints, 0,
        "a stored login should make the storefront irrelevant"
    );
}

#[tokio::test]
async fn a_wrong_password_is_reported_as_such() {
    let f = Fixture::start().await;
    let cp = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/apigee-credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "a" })))
        .mount(&cp)
        .await;
    Mock::given(method("POST"))
        .and(path("/user/login"))
        .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
        .mount(&cp)
        .await;

    let mut cmd = f.cmd();
    let out = with_clubplus(&mut cmd, &cp.uri())
        .args([
            "auth",
            "login",
            "--email",
            "s@example.test",
            "--password-command",
            "echo wrong",
        ])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("rejected that email and password"),
        "got: {stderr}"
    );
}

#[tokio::test]
async fn an_unexpected_token_response_names_the_fields_it_saw() {
    let f = Fixture::start().await;
    let cp = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/apigee-credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "a" })))
        .mount(&cp)
        .await;
    Mock::given(method("POST"))
        .and(path("/user/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "access_token": "cp", "refresh_token": "r", "isEmailVerified": true }),
        ))
        .mount(&cp)
        .await;
    // A secure-token response in a shape the parser does not recognise.
    Mock::given(method("POST"))
        .and(path("/user/token/secure"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "sessionRef": "x", "ttl": 1800 })),
        )
        .mount(&cp)
        .await;

    let mut cmd = f.cmd();
    let out = with_clubplus(&mut cmd, &cp.uri())
        .args([
            "auth",
            "login",
            "--email",
            "s@example.test",
            "--password-command",
            "echo pw",
        ])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("sessionRef"),
        "should name what it saw: {stderr}"
    );
    assert!(stderr.contains("ttl"), "got: {stderr}");
}

#[tokio::test]
async fn logout_forgets_the_login_and_the_cached_tokens() {
    let f = Fixture::start().await;
    let cp = MockServer::start().await;
    f.mount_login(&cp).await;

    let mut cmd = f.cmd_with_stores();
    with_clubplus(&mut cmd, &cp.uri())
        .args([
            "auth",
            "login",
            "--email",
            "s@example.test",
            "--password-command",
            "echo pw",
        ])
        .output()
        .unwrap();

    let out = f.cmd().args(["--json", "auth", "logout"]).output().unwrap();
    assert!(out.status.success());
    assert_eq!(stdout_json(&out)["had_login"], true);

    let out = f.cmd().args(["--json", "auth", "logout"]).output().unwrap();
    assert_eq!(
        stdout_json(&out)["had_login"],
        false,
        "logout is idempotent"
    );
}

#[tokio::test]
async fn doctor_reports_who_is_logged_in() {
    let f = Fixture::start().await;
    let cp = MockServer::start().await;
    f.mount_login(&cp).await;

    let mut cmd = f.cmd_with_stores();
    with_clubplus(&mut cmd, &cp.uri())
        .args([
            "auth",
            "login",
            "--email",
            "shopper@example.test",
            "--password-command",
            "echo pw",
        ])
        .output()
        .unwrap();

    let out = f
        .cmd_with_stores()
        .args(["--json", "doctor"])
        .output()
        .unwrap();
    assert_eq!(stdout_json(&out)["logged_in_as"], "shopper@example.test");
}

#[tokio::test]
async fn login_will_not_hang_waiting_for_a_prompt_in_a_script() {
    let f = Fixture::start().await;
    let cp = MockServer::start().await;
    f.mount_login(&cp).await;

    // No --email and no terminal: it must fail fast, not block forever.
    let mut cmd = f.cmd();
    let out = with_clubplus(&mut cmd, &cp.uri())
        .args(["auth", "login", "--password-command", "echo pw"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no terminal"));
}
