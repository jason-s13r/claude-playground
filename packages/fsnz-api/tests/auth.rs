//! The Club Plus chain against mock servers.
//!
//! Two servers throughout, because the load-bearing fact about this chain is
//! *which host* step 3 goes to.

use fsnz_api::auth::clubplus::{self, Config, Login};
use fsnz_api::{Banner, ClubPlusEndpoints, Endpoints};
use net_kit::ClientSpec;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn http() -> net_kit::wreq::Client {
    net_kit::http::build(ClientSpec::new(
        fsnz_api::EMULATION,
        net_kit::wreq::redirect::Policy::none(),
    ))
    .expect("building a client")
}

/// Club Plus hands out its apigee key to anyone who asks.
async fn mount_apigee(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/apigee-credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "apigee" })))
        .mount(server)
        .await;
}

fn endpoints(clubplus: &MockServer) -> ClubPlusEndpoints {
    ClubPlusEndpoints::default()
        .with_login(clubplus.uri())
        .with_api(clubplus.uri())
}

#[tokio::test]
async fn a_password_login_returns_a_session() {
    let server = MockServer::start().await;
    mount_apigee(&server).await;
    Mock::given(method("POST"))
        .and(path("/user/login"))
        .and(header("x-device-id", "device-1"))
        .and(body_partial_json(
            json!({ "email": "shopper@example.test", "source": "WEB" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "cp-access", "refresh_token": "cp-refresh", "isEmailVerified": true
        })))
        .mount(&server)
        .await;

    let http = http();
    let cp = endpoints(&server);
    let cfg = Config {
        http: &http,
        clubplus: &cp,
        device_id: "device-1",
    };

    match clubplus::login(&cfg, "shopper@example.test", "hunter2")
        .await
        .unwrap()
    {
        Login::Complete(session) => {
            assert_eq!(session.access_token, "cp-access");
            assert_eq!(session.refresh_token.as_deref(), Some("cp-refresh"));
        }
        Login::ChallengeRequired(_) => panic!("no code was asked for"),
    }
}

#[tokio::test]
async fn an_unrecognised_device_is_asked_for_the_emailed_code() {
    let server = MockServer::start().await;
    mount_apigee(&server).await;
    Mock::given(method("POST"))
        .and(path("/user/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "pre-auth",
            "isTFARequired": true,
            "tfaMethod": "EMAIL_OTP",
            "phvToken": "phv-1"
        })))
        .mount(&server)
        .await;
    // The code is redeemed with the pre-auth token, not the apigee one, and
    // carries no x-device-id: phvToken already pins the device.
    Mock::given(method("POST"))
        .and(path("/user/tfa/login"))
        .and(header("authorization", "Bearer pre-auth"))
        .and(body_partial_json(
            json!({ "code": "123456", "phvToken": "phv-1" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "cp-access", "refresh_token": "cp-refresh"
        })))
        .mount(&server)
        .await;

    let http = http();
    let cp = endpoints(&server);
    let cfg = Config {
        http: &http,
        clubplus: &cp,
        device_id: "device-1",
    };

    let challenge = match clubplus::login(&cfg, "shopper@example.test", "hunter2")
        .await
        .unwrap()
    {
        Login::ChallengeRequired(c) => c,
        Login::Complete(_) => panic!("a code should have been required"),
    };
    assert_eq!(challenge.method, "EMAIL_OTP");

    let session = clubplus::complete_challenge(&cfg, &challenge, " 123456 ")
        .await
        .unwrap();
    assert_eq!(session.access_token, "cp-access");
}

#[tokio::test]
async fn a_refused_refresh_token_is_its_own_error_not_a_generic_401() {
    let server = MockServer::start().await;
    mount_apigee(&server).await;
    Mock::given(method("POST"))
        .and(path("/user/login/refresh"))
        .respond_with(ResponseTemplate::new(401).set_body_string("expired"))
        .mount(&server)
        .await;

    let http = http();
    let cp = endpoints(&server);
    let cfg = Config {
        http: &http,
        clubplus: &cp,
        device_id: "device-1",
    };

    let err = clubplus::refresh(&cfg, "spent-token").await.unwrap_err();
    assert!(matches!(err, fsnz_api::Error::RefreshRejected), "{err:?}");
}

#[tokio::test]
async fn a_refresh_that_returns_no_new_token_keeps_the_old_one() {
    // Dropping it would cost the ability to renew again.
    let server = MockServer::start().await;
    mount_apigee(&server).await;
    Mock::given(method("POST"))
        .and(path("/user/login/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "new" })))
        .mount(&server)
        .await;

    let http = http();
    let cp = endpoints(&server);
    let cfg = Config {
        http: &http,
        clubplus: &cp,
        device_id: "device-1",
    };

    let session = clubplus::refresh(&cfg, "still-good").await.unwrap();
    assert_eq!(session.refresh_token.as_deref(), Some("still-good"));
}

#[tokio::test]
async fn the_secure_token_is_issued_by_club_plus_and_never_by_the_banner() {
    // The load-bearing fact of this whole chain. The banner API answers
    // /user/token/secure with 200 and a plausible token, but the code it issues
    // scopes back to NAT -- and a NAT token is not refused by the cart, it just
    // answers with an empty one belonging to nobody. So: two servers, and the
    // banner must see zero requests for it.
    let clubplus_server = MockServer::start().await;
    let banner_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/user/token/secure"))
        .and(header("authorization", "Bearer cp-access"))
        .and(body_partial_json(
            json!({ "banner": "MNW", "source": "WEB" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "secure_token": "uuid-1" })))
        .mount(&clubplus_server)
        .await;

    // If the implementation ever sent step 3 to the banner, this would answer
    // it -- and the assertion below would catch that it did.
    Mock::given(method("POST"))
        .and(path("/user/token/secure"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "secure_token": "WRONG" })))
        .mount(&banner_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/user/login/sso"))
        .and(body_partial_json(
            json!({ "key": "uuid-1", "forceNewSession": false }),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "access_token": "banner.jwt" })),
        )
        .mount(&banner_server)
        .await;

    let http = http();
    let cp = endpoints(&clubplus_server);
    let cfg = Config {
        http: &http,
        clubplus: &cp,
        device_id: "device-1",
    };
    let endpoints = Endpoints::defaults(Banner::NewWorld).with_origin(banner_server.uri());
    let session = clubplus::Session {
        access_token: "cp-access".into(),
        refresh_token: None,
    };

    let token = clubplus::banner_token(&cfg, Banner::NewWorld, &endpoints, &session, "UA/1.0")
        .await
        .unwrap();
    assert_eq!(token, "banner.jwt");

    let to_banner = banner_server.received_requests().await.unwrap();
    assert!(
        to_banner
            .iter()
            .all(|r| r.url.path() != "/user/token/secure"),
        "step 3 must go to Club Plus; the banner answers it with a NAT-scoped token"
    );
    assert_eq!(to_banner.len(), 1, "the banner is only asked to exchange");
}

#[tokio::test]
async fn the_sso_exchange_echoes_the_user_agent_it_was_given() {
    // It travels in the body, not a header, and has to be the one the
    // handshake implies.
    let clubplus_server = MockServer::start().await;
    let banner_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/user/token/secure"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "secure_token": "uuid-1" })))
        .mount(&clubplus_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/user/login/sso"))
        .and(body_partial_json(
            json!({ "fingerprintGuest": "Mozilla/5.0 Test" }),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "accessToken": "banner.jwt" })),
        )
        .mount(&banner_server)
        .await;

    let http = http();
    let cp = endpoints(&clubplus_server);
    let cfg = Config {
        http: &http,
        clubplus: &cp,
        device_id: "d",
    };
    let endpoints = Endpoints::defaults(Banner::NewWorld).with_origin(banner_server.uri());
    let session = clubplus::Session {
        access_token: "cp".into(),
        refresh_token: None,
    };

    let token = clubplus::banner_token(
        &cfg,
        Banner::NewWorld,
        &endpoints,
        &session,
        "Mozilla/5.0 Test",
    )
    .await
    .unwrap();
    assert_eq!(token, "banner.jwt");
}

#[tokio::test]
async fn a_cloudflare_interstitial_is_not_reported_as_an_auth_failure() {
    use net_kit::Fault;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/apigee-credentials"))
        .respond_with(ResponseTemplate::new(403).set_body_string(
            "<!DOCTYPE html><html><head><title>Just a moment...</title></head></html>",
        ))
        .mount(&server)
        .await;

    let http = http();
    let cp = endpoints(&server);
    let cfg = Config {
        http: &http,
        clubplus: &cp,
        device_id: "d",
    };

    let err = clubplus::apigee_token(&cfg).await.unwrap_err();
    assert!(matches!(err, fsnz_api::Error::Challenged { .. }), "{err:?}");
    // A renewal cannot clear a bot check; reporting it as auth would spend one.
    assert_eq!(err.auth(), None);
}

#[tokio::test]
async fn a_wrong_password_carries_the_status_and_body_through() {
    let server = MockServer::start().await;
    mount_apigee(&server).await;
    Mock::given(method("POST"))
        .and(path("/user/login"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({ "message": "Invalid credentials" })),
        )
        .mount(&server)
        .await;

    let http = http();
    let cp = endpoints(&server);
    let cfg = Config {
        http: &http,
        clubplus: &cp,
        device_id: "d",
    };

    let err = clubplus::login(&cfg, "shopper@example.test", "wrong")
        .await
        .unwrap_err();
    assert!(err.body().contains("Invalid credentials"), "{err}");
    assert_eq!(
        net_kit::Fault::auth(&err),
        Some(net_kit::AuthFault::Rejected)
    );
}

#[tokio::test]
async fn an_unverified_email_is_refused_rather_than_treated_as_a_session() {
    let server = MockServer::start().await;
    mount_apigee(&server).await;
    Mock::given(method("POST"))
        .and(path("/user/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "cp-access", "isEmailVerified": false
        })))
        .mount(&server)
        .await;

    let http = http();
    let cp = endpoints(&server);
    let cfg = Config {
        http: &http,
        clubplus: &cp,
        device_id: "d",
    };

    let err = clubplus::login(&cfg, "shopper@example.test", "hunter2")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not verified"), "{err}");
}
