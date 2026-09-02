//! A stand-in for Foodstuffs: one mock server per banner, serving both the
//! storefront (which mints the guest token) and the JSON API.
// Each integration test binary compiles this module but uses only part of it.
#![allow(dead_code)]

use base64::Engine;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const NW_STORE: &str = "nw-store-1";
pub const PNS_STORE: &str = "pns-store-1";

pub struct Fixture {
    pub newworld: MockServer,
    pub paknsave: MockServer,
    pub home: TempDir,
}

/// A JWT whose payload carries only `exp`.
pub fn jwt(expires_in_secs: u64) -> String {
    jwt_with(expires_in_secs, json!({}))
}

/// A JWT carrying `exp` plus whatever claims a test cares about -- `banner` and
/// `linkedAccounts` are the ones `fsnz auth status` reads.
pub fn jwt_with(expires_in_secs: u64, extra: serde_json::Value) -> String {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + expires_in_secs;
    let mut claims = json!({ "exp": exp });
    if let Some(extra) = extra.as_object() {
        for (k, v) in extra {
            claims[k] = v.clone();
        }
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
    format!("header.{payload}.signature")
}

/// A Club Plus session token: national scope, linked to New World only, which
/// is the shape a real account has.
pub fn clubplus_jwt(expires_in_secs: u64) -> String {
    jwt_with(
        expires_in_secs,
        json!({ "banner": "NAT", "linkedAccounts": [{ "banner": "MNW" }] }),
    )
}

impl Fixture {
    pub async fn start() -> Fixture {
        let fixture = Fixture {
            newworld: MockServer::start().await,
            paknsave: MockServer::start().await,
            home: TempDir::new().expect("temp dir"),
        };
        mount_storefront(&fixture.newworld).await;
        mount_storefront(&fixture.paknsave).await;
        mount_stores(
            &fixture.newworld,
            json!([
                { "id": NW_STORE, "name": "New World Thorndon", "region": "Wellington" },
                { "id": "nw-store-2", "name": "New World Karori", "region": "Wellington" },
            ]),
        )
        .await;
        mount_stores(
            &fixture.paknsave,
            // Wrapped shape, to prove both are handled.
            json!({ "stores": [
                { "id": PNS_STORE, "name": "PAK'nSAVE Kilbirnie", "region": "Wellington" },
            ]}),
        )
        .await;
        fixture
    }

    /// Mount a working Club Plus login on `server`: the credentials, the
    /// password exchange and the secure-token step all live there, and only the
    /// final sso exchange happens at each banner's storefront.
    pub async fn mount_login(&self, server: &MockServer) {
        self.mount_login_lasting(server, 1800).await
    }

    /// As [`Fixture::mount_login`], but the session it hands out is already
    /// past its expiry, so the next command has to renew before it can mint.
    pub async fn mount_expired_login(&self, server: &MockServer) {
        self.mount_login_lasting(server, 0).await
    }

    async fn mount_login_lasting(&self, server: &MockServer, session_secs: u64) {
        Mock::given(method("GET"))
            .and(path("/api/apigee-credentials"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "access_token": "apigee-public-token", "expires_in": "3599" }),
            ))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/user/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": clubplus_jwt(session_secs),
                "refresh_token": "clubplus-refresh",
                "isEmailVerified": true,
            })))
            .mount(server)
            .await;
        // Renewal. The refresh token is rotated, so the replacement here is a
        // different value on purpose: a client that keeps sending the old one
        // would pass a mock that echoed it back.
        Mock::given(method("POST"))
            .and(path("/user/login/refresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": clubplus_jwt(1800),
                "refresh_token": "clubplus-refresh-2",
                "isEmailVerified": true,
            })))
            .mount(server)
            .await;
        // Step one hands back an exchange code, not a token. It is Club Plus
        // that issues it: a code minted by the banner API exchanges back into a
        // national token, which the cart answers with an empty cart.
        Mock::given(method("POST"))
            .and(path("/user/token/secure"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({ "secure_token": "00000000-0000-4000-8000-000000000001" }),
                ),
            )
            .mount(server)
            .await;
        // Step two swaps the code for the token, at the storefront. The token
        // carries the banner it was scoped to, which is what `auth status`
        // reports and what tells an account token from a national one.
        for (banner, code) in [(&self.newworld, "MNW"), (&self.paknsave, "PNS")] {
            Mock::given(method("POST"))
                .and(path("/api/user/login/sso"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": jwt_with(1800, json!({ "banner": code })),
                    "expires_in": 1800,
                })))
                .mount(banner)
                .await;
        }
    }

    /// Base command with the fixture's endpoints and a private config/state dir.
    pub fn cmd(&self) -> assert_cmd::Command {
        self.command(assert_cmd::Command::cargo_bin("fsnz").expect("binary built"))
    }

    /// As [`Fixture::cmd`], but running a particular copy of the binary. The
    /// update tests need this: `fsnz update` replaces the file it is running
    /// from, which must not be the one cargo just built.
    pub fn cmd_at(&self, path: &std::path::Path) -> assert_cmd::Command {
        self.command(assert_cmd::Command::new(path))
    }

    fn command(&self, mut cmd: assert_cmd::Command) -> assert_cmd::Command {
        for key in [
            "FSNZ_BANNER",
            "FSNZ_TOKEN",
            "FSNZ_NEWWORLD_STORE_ID",
            "FSNZ_PAKNSAVE_STORE_ID",
            "FSNZ_UPDATE_API",
        ] {
            cmd.env_remove(key);
        }
        cmd.env("NO_COLOR", "1")
            .env("FSNZ_SECRET_BACKEND", "file")
            .env("FSNZ_CONFIG_DIR", self.home.path().join("config"))
            .env("FSNZ_STATE_DIR", self.home.path().join("state"))
            .env("FSNZ_NEWWORLD_ORIGIN", self.newworld.uri())
            .env("FSNZ_NEWWORLD_API", self.newworld.uri())
            .env("FSNZ_PAKNSAVE_ORIGIN", self.paknsave.uri())
            .env("FSNZ_PAKNSAVE_API", self.paknsave.uri());
        cmd
    }

    /// As [`Fixture::cmd`], with both stores already selected.
    pub fn cmd_with_stores(&self) -> assert_cmd::Command {
        let mut cmd = self.cmd();
        cmd.env("FSNZ_NEWWORLD_STORE_ID", NW_STORE)
            .env("FSNZ_PAKNSAVE_STORE_ID", PNS_STORE);
        cmd
    }
}

pub async fn mount_storefront(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "set-cookie",
                    format!("fs-user-token={}; Path=/; HttpOnly", jwt(1800)).as_str(),
                )
                .set_body_string("<html></html>"),
        )
        .mount(server)
        .await;
}

pub async fn mount_stores(server: &MockServer, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path("/v1/edge/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// A cart body in the shape the API returns.
pub fn cart_body(items: Vec<serde_json::Value>) -> serde_json::Value {
    let subtotal: i64 = items.iter().map(|i| i["price"].as_i64().unwrap_or(0)).sum();
    json!({
        "products": items,
        "unavailableProducts": [],
        "subtotal": subtotal,
        "serviceFee": 0,
        "bagFee": 150,
        "bagless": false,
        "promoCodeDiscount": 0,
        "subscriptionDiscount": 0,
        "clubMember": true,
        "whenLastPriced": "2026-08-29T23:01:57.135+12:00",
        "store": { "storeId": NW_STORE, "storeName": "New World Thorndon", "storeRegion": "NI" }
    })
}

pub fn cart_item(
    sku: &str,
    name: &str,
    qty: u32,
    sale_type: &str,
    cents: i64,
) -> serde_json::Value {
    json!({
        "productId": sku, "name": name, "quantity": qty,
        "sale_type": sale_type, "price": cents,
        "isLiquor": false, "isTobacco": false, "isCatered": false,
        "originStatement": "Product of New Zealand"
    })
}

/// One product in the shape the search endpoint returns.
pub fn product(sku: &str, name: &str, brand: &str, size: &str, cents: i64) -> serde_json::Value {
    json!({
        "productId": sku,
        "name": name,
        "brand": brand,
        "displayName": size,
        "availability": ["ONLINE", "INSTORE"],
        "singlePrice": {
            "price": cents,
            "comparativePrice": { "pricePerUnit": cents / 2, "measureDescription": "1L" }
        },
        "categoryTrees": [{ "level0": "Chilled, Frozen & Desserts" }]
    })
}

/// A product carrying a promotion, which is how a special is signalled.
pub fn special(sku: &str, name: &str, brand: &str, size: &str, cents: i64) -> serde_json::Value {
    let mut p = product(sku, name, brand, size, cents);
    p["singlePrice"]["promoId"] = json!("PROMO-1");
    p["promotions"] = json!([{ "threshold": 2, "rewardValue": 700 }]);
    p
}

pub fn search_response(products: Vec<serde_json::Value>) -> serde_json::Value {
    let total = products.len();
    json!({ "products": products, "totalHits": total, "totalPages": 1 })
}

/// Mock the product search endpoint with one canned page.
pub async fn mount_search(server: &MockServer, body: Value) {
    Mock::given(method("POST"))
        .and(path("/v1/edge/search/paginated/products"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

pub fn stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should be JSON")
}

/// A stand-in for the GitHub releases API, plus the host the assets download
/// from. Both are the same server here; on the real thing the assets live on a
/// separate domain, which is why the client follows the URL the API hands it
/// rather than building one.
pub struct Github {
    pub server: MockServer,
}

impl Github {
    pub async fn start() -> Github {
        Github {
            server: MockServer::start().await,
        }
    }

    /// Mount the release list. Order is deliberately not sorted here -- picking
    /// the newest is the client's job.
    pub async fn releases(&self, body: Value) {
        Mock::given(method("GET"))
            .and(path("/repos/jason-s13r/claude-playground/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    pub async fn releases_status(&self, status: u16, body: &str) {
        Mock::given(method("GET"))
            .and(path("/repos/jason-s13r/claude-playground/releases"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .mount(&self.server)
            .await;
    }

    pub async fn asset(&self, name: &str, bytes: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path(format!("/download/{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(&self.server)
            .await;
    }

    /// An asset served the way GitHub serves one: a 302 from the download URL
    /// to the host that actually holds the bytes.
    pub async fn asset_redirecting(&self, name: &str, bytes: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path(format!("/download/{name}")))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", format!("/blob/{name}")),
            )
            .mount(&self.server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/blob/{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(&self.server)
            .await;
    }

    pub fn download_url(&self, name: &str) -> String {
        format!("{}/download/{name}", self.server.uri())
    }

    /// One entry in the release list, in the shape GitHub returns.
    pub fn release(&self, tag: &str, assets: &[String]) -> Value {
        json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/jason-s13r/claude-playground/releases/tag/{tag}"),
            "draft": false,
            "prerelease": tag.contains("-rc"),
            "assets": assets.iter().map(|name| json!({
                "name": name,
                "browser_download_url": self.download_url(name),
            })).collect::<Vec<_>>(),
        })
    }
}

/// The asset names `make dist` would produce for this host, in the same
/// `<project>-<version>-<os>-<arch>.tar.gz` shape.
///
/// The mapping from Rust's target names to `uname`'s is the same one the
/// client makes; a test that hardcoded one platform would only ever exercise
/// the machine it was written on.
pub fn host_asset_names(version: &str) -> Vec<String> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arches: Vec<&str> = match std::env::consts::ARCH {
        "aarch64" => vec!["arm64", "aarch64"],
        "x86_64" => vec!["x86_64", "amd64"],
        other => vec![other],
    };
    arches
        .iter()
        .map(|arch| format!("foodstuffs-nz-cli-{version}-{os}-{arch}.tar.gz"))
        .collect()
}

/// A gzipped tar holding one executable file, which is what `make dist` builds.
pub fn tarball(name: &str, contents: &[u8]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    builder.append_data(&mut header, name, contents).unwrap();
    builder.into_inner().unwrap().finish().unwrap()
}

pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A `sha256sum`-format checksum file covering `files`.
pub fn sha256sums(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    files
        .iter()
        .map(|(name, bytes)| format!("{}  {name}\n", sha256(bytes)))
        .collect::<String>()
        .into_bytes()
}
