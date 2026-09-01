//! `wwnz auth` -- the login flow, importing a session, and reporting on one.

mod support;

use predicates::prelude::*;
use serde_json::json;
use support::{stdout_json, Fixture};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The Auth0 login pages, in the shape the real flow serves them.
///
/// The chain is: the storefront redirects to the identifier form, that posts to
/// the password form, and that redirects back to a callback which sets the
/// session cookie. Each form echoes a `state` the next step has to send back.
async fn mount_login(server: &MockServer, password: &str) {
    let password = password.to_string();
    let form = |state: &str, action: &str| {
        format!(
            r#"<html><body><form method="post" action="{action}">
                 <input type="hidden" name="state" value="{state}">
                 <input name="username" type="email">
               </form></body></html>"#
        )
    };

    // Step one: /auth/login redirects to the identifier form. The mock serves
    // the form directly, which is what the client sees after redirects.
    Mock::given(method("GET"))
        .and(path("/auth/login"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(form("state-one", "/u/login/identifier")),
        )
        .mount(server)
        .await;

    // Step two: the email is accepted and the password form is served, with a
    // fresh state -- reusing the first one fails against the real Auth0.
    Mock::given(method("POST"))
        .and(path("/u/login/identifier"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(form("state-two", "/u/login/password")),
        )
        .mount(server)
        .await;

    // Step three: the right password ends the flow with the session cookie;
    // the wrong one re-renders the form carrying Auth0's error banner.
    Mock::given(method("POST"))
        .and(path("/u/login/password"))
        .respond_with(move |req: &wiremock::Request| {
            let body = String::from_utf8_lossy(&req.body);
            let sent = form_field(&body, "password");
            if sent.as_deref() == Some(password.as_str()) {
                ResponseTemplate::new(200)
                    .append_header("set-cookie", "__session__0=half-one; Path=/")
                    .append_header("set-cookie", "__session__1=half-two; Path=/")
                    .set_body_string("<html>signed in</html>")
            } else {
                ResponseTemplate::new(400).set_body_string(
                    r#"<html><span id="error-element-password"
                       class="ulp-input-error-message">Wrong email or password</span></html>"#,
                )
            }
        })
        .mount(server)
        .await;
}

/// One field out of a urlencoded form body.
fn form_field(body: &str, name: &str) -> Option<String> {
    body.split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| {
            // Only what these tests actually put in a password.
            v.replace('+', " ")
                .replace("%40", "@")
                .replace("%21", "!")
                .replace("%2A", "*")
        })
}

#[tokio::test]
async fn logging_in_stores_a_session_that_later_commands_use() {
    let f = Fixture::start().await;
    mount_login(&f.server, "hunter2").await;
    f.mount_op(
        "Orders",
        json!({ "orders": {
            "results": [], "totalCount": 0, "totalPages": 0,
        }}),
    )
    .await;

    f.cmd()
        .args([
            "auth",
            "login",
            "--email",
            "shopper@example.test",
            "--password-command",
            "printf hunter2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Signed in as shopper@example.test",
        ));

    // The stored session is what makes the next command account-scoped; no
    // WWNZ_SESSION is set here.
    f.cmd()
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Signed in as shopper@example.test",
        ))
        .stdout(predicate::str::contains("The session works"));
}

#[tokio::test]
async fn a_wrong_password_is_reported_with_the_reason_the_page_gave() {
    let f = Fixture::start().await;
    mount_login(&f.server, "hunter2").await;

    f.cmd()
        .args([
            "auth",
            "login",
            "--email",
            "shopper@example.test",
            "--password-command",
            "printf wrong",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wrong email or password"))
        // And it points at the way in when the flow itself is the problem.
        .stderr(predicate::str::contains("wwnz auth import"));
}

#[tokio::test]
async fn a_login_page_that_carries_no_form_is_reported_as_such() {
    let f = Fixture::start().await;
    // What a bot check looks like: a 200 that is not the login page.
    Mock::given(method("GET"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>Access Denied</html>"))
        .mount(&f.server)
        .await;

    f.cmd()
        .args([
            "auth",
            "login",
            "--email",
            "shopper@example.test",
            "--password-command",
            "printf hunter2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("did not carry a login form"))
        .stderr(predicate::str::contains("wwnz auth import"));
}

#[tokio::test]
async fn a_session_can_be_imported_from_exported_cookies() {
    let f = Fixture::start().await;
    f.mount_op(
        "Orders",
        json!({ "orders": {
            "results": [], "totalCount": 0, "totalPages": 0,
        }}),
    )
    .await;

    let cookies = f.home.path().join("cookies.txt");
    std::fs::write(
        &cookies,
        "# Netscape HTTP Cookie File\n\
         www.woolworths.co.nz\tFALSE\t/\tTRUE\t1788386095\t__session__0\thalf-one\n\
         #HttpOnly_www.woolworths.co.nz\tFALSE\t/\tTRUE\t1788386095\t__session__1\thalf-two\n\
         .woolworths.co.nz\tTRUE\t/\tTRUE\t1788386095\tbm_sv\tnot-a-credential\n",
    )
    .unwrap();

    f.cmd()
        .args(["auth", "import", cookies.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Session imported"));

    f.cmd()
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("The session works"));
}

#[tokio::test]
async fn importing_a_file_with_no_session_in_it_is_refused() {
    let f = Fixture::start().await;
    let cookies = f.home.path().join("cookies.txt");
    std::fs::write(
        &cookies,
        "www.newworld.co.nz\tFALSE\t/\tTRUE\t1788386095\tAPI_TOKEN\tsomeone-elses\n",
    )
    .unwrap();

    f.cmd()
        .args(["auth", "import", cookies.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no Woolworths session found"));
}

#[tokio::test]
async fn status_reports_not_signed_in_and_exits_non_zero() {
    let f = Fixture::start().await;

    f.cmd()
        .args(["auth", "status"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("Not signed in"));

    let out = f
        .cmd()
        .args(["auth", "status", "--json"])
        .output()
        .expect("run");
    assert_eq!(stdout_json(&out)["signed_in"], json!(false));
}

#[tokio::test]
async fn status_says_when_a_stored_session_no_longer_works() {
    let f = Fixture::start().await;
    f.mount_op_error(
        "Orders",
        "The current user is not authorized to access this resource.",
        "AUTH_NOT_AUTHENTICATED",
    )
    .await;

    // A session that exists but is refused is the single most useful thing
    // this command can find, and it must not be reported as "signed in, fine".
    f.cmd_signed_in()
        .args(["auth", "status"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("The session no longer works"))
        .stdout(predicate::str::contains("wwnz auth login"));
}

#[tokio::test]
async fn logging_out_forgets_the_session() {
    let f = Fixture::start().await;
    mount_login(&f.server, "hunter2").await;
    f.mount_op(
        "Orders",
        json!({ "orders": {
            "results": [], "totalCount": 0, "totalPages": 0,
        }}),
    )
    .await;

    f.cmd()
        .args([
            "auth",
            "login",
            "--email",
            "shopper@example.test",
            "--password-command",
            "printf hunter2",
        ])
        .assert()
        .success();

    f.cmd()
        .args(["auth", "logout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Signed out."));

    f.cmd()
        .args(["auth", "status"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("Not signed in"));

    // Logging out twice is not an error.
    f.cmd()
        .args(["auth", "logout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no stored session"));
}

#[tokio::test]
async fn the_session_env_var_overrides_anything_stored() {
    let f = Fixture::start().await;
    f.mount_op(
        "Orders",
        json!({ "orders": {
            "results": [], "totalCount": 0, "totalPages": 0,
        }}),
    )
    .await;

    f.cmd_signed_in()
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("WWNZ_SESSION"));
}
