//! End-to-end: minting, caching and refreshing the guest token.
//!
//! Nothing here touches the internet -- both banners' endpoints are pointed at
//! a local server, which is also what makes the request bodies assertable.

mod support;

use support::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn a_rejected_token_suggests_refreshing_it() {
    let f = Fixture::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorised"))
        .mount(&f.newworld)
        .await;

    let out = f
        .cmd_with_stores()
        .args(["search", "milk"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("401"), "got: {stderr}");
    assert!(stderr.contains("token --refresh"), "got: {stderr}");
}

#[tokio::test]
async fn a_storefront_that_sets_no_cookie_explains_the_workarounds() {
    let f = Fixture::start().await;
    let bare = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(403).set_body_string("blocked"))
        .mount(&bare)
        .await;

    let out = f
        .cmd_with_stores()
        .env("FSNZ_NEWWORLD_ORIGIN", bare.uri())
        .args(["search", "milk"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("fs-user-token"), "got: {stderr}");
    assert!(stderr.contains("fsnz auth login"), "got: {stderr}");
    assert!(stderr.contains("FSNZ_TOKEN"), "got: {stderr}");
}

#[tokio::test]
async fn a_cloudflare_challenge_is_named_rather_than_guessed_at() {
    let f = Fixture::start().await;
    let challenged = MockServer::start().await;
    // What www.newworld.co.nz actually returns to a non-browser client.
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("cf-mitigated", "challenge")
                .set_body_string("<html>challenge</html>"),
        )
        .mount(&challenged)
        .await;

    let out = f
        .cmd_with_stores()
        .env("FSNZ_NEWWORLD_ORIGIN", challenged.uri())
        .args(["search", "milk"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no fs-user-token cookie"), "got: {stderr}");
    assert!(stderr.contains("FSNZ_TOKEN"), "got: {stderr}");
    // Retrying is pointless, so the message must not suggest it.
    assert!(
        !stderr.to_lowercase().contains("try again"),
        "got: {stderr}"
    );
}

#[tokio::test]
async fn the_guest_token_is_minted_once_and_then_cached() {
    let f = Fixture::start().await;
    mount_search(&f.newworld, search_response(vec![])).await;

    for _ in 0..3 {
        let out = f
            .cmd_with_stores()
            .args(["search", "milk"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let mints = f
        .newworld
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/")
        .count();
    assert_eq!(mints, 1, "the cached token should be reused across runs");
}

#[tokio::test]
async fn token_refresh_goes_back_to_the_storefront() {
    let f = Fixture::start().await;

    f.cmd_with_stores()
        .args(["auth", "token"])
        .output()
        .unwrap();
    let out = f
        .cmd_with_stores()
        .args(["--json", "auth", "token", "--refresh"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(stdout_json(&out)["source"], "minted from the storefront");

    let mints = f
        .newworld
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/")
        .count();
    assert_eq!(mints, 2);
}

#[tokio::test]
async fn an_explicit_token_skips_the_storefront_entirely() {
    let f = Fixture::start().await;
    mount_search(&f.newworld, search_response(vec![])).await;

    let out = f
        .cmd_with_stores()
        .args(["--token", &jwt(600), "search", "milk"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mints = f
        .newworld
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/")
        .count();
    assert_eq!(mints, 0);
}

#[tokio::test]
async fn requests_carry_the_bearer_token() {
    let f = Fixture::start().await;
    mount_search(&f.newworld, search_response(vec![])).await;
    let token = jwt(600);

    f.cmd_with_stores()
        .args(["--token", &token, "search", "milk"])
        .output()
        .unwrap();

    let requests = f.newworld.received_requests().await.unwrap();
    let search = requests
        .iter()
        .find(|r| r.url.path() == "/v1/edge/search/paginated/products")
        .expect("search request");
    assert_eq!(
        search.headers.get("authorization").unwrap(),
        &format!("Bearer {token}")
    );
}
