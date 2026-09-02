//! A stand-in for Woolworths: one mock server serving the storefront (which
//! mints the guest token) and the GraphQL endpoint.
// Each integration test binary compiles this module but uses only part of it.
#![allow(dead_code)]

use serde_json::{json, Value};
use tempfile::TempDir;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const STORE_ID: &str = "9048";
pub const STORE_NAME: &str = "Regent Woolworths";

/// A session cookie header, in the split form the site actually sets. Nothing
/// here can decrypt it, and the mock does not care what is inside -- it only
/// has to be present for a request to count as signed in.
pub const SESSION: &str = "__session__0=encrypted-half-one; __session__1=encrypted-half-two";

pub struct Fixture {
    pub server: MockServer,
    pub home: TempDir,
}

impl Fixture {
    pub async fn start() -> Fixture {
        let fixture = Fixture::start_bare().await;
        fixture.mount_storefront().await;
        fixture
    }

    /// A fixture with no storefront mounted, so the guest-token bootstrap can
    /// be given something other than a working answer.
    pub async fn start_bare() -> Fixture {
        Fixture {
            server: MockServer::start().await,
            home: TempDir::new().expect("temp dir"),
        }
    }

    /// The storefront page, which exists only to hand out a guest token.
    pub async fn mount_storefront(&self) {
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        "set-cookie",
                        "__guest__token=a-guest-token; Path=/; HttpOnly",
                    )
                    .set_body_string("<html></html>"),
            )
            .mount(&self.server)
            .await;
    }

    /// Answer one GraphQL operation with a canned body.
    ///
    /// Routing is on the `op-name` query parameter, which the client sends
    /// alongside the operation in the document. It is the only thing that
    /// distinguishes one POST to `/api/graphql` from another.
    pub async fn mount_op(&self, operation: &str, data: Value) {
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(query_param("op-name", operation))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": data })))
            .mount(&self.server)
            .await;
    }

    /// Answer an operation with a GraphQL error, which is how this API reports
    /// failure -- a 200 carrying `errors`.
    pub async fn mount_op_error(&self, operation: &str, message: &str, code: &str) {
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(query_param("op-name", operation))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "errors": [{ "message": message, "extensions": { "code": code } }],
            })))
            .mount(&self.server)
            .await;
    }

    /// The store lookup, which nearly every command reaches through.
    pub async fn mount_stores(&self, locations: Value) {
        self.mount_op(
            "SearchLocations",
            json!({ "locations": { "locations": locations } }),
        )
        .await;
    }

    /// The default store list, and the mutation that binds one to the cart.
    pub async fn mount_store_selection(&self) {
        self.mount_stores(json!([
            location(STORE_ID, STORE_NAME, "Regent, Whangarei"),
            location("9195", "Whangarei Woolworths", "Whangarei"),
        ]))
        .await;
        self.mount_op(
            "SetCartShoppingMode",
            json!({ "setCartShoppingMode": {
                "shoppingMode": { "pickupLocation": { "id": STORE_ID, "name": STORE_NAME } },
                "validationResult": { "isValid": true, "failedValidations": [] },
            }}),
        )
        .await;
    }

    pub async fn mount_search(&self, results: Vec<Value>, total: u32) {
        self.mount_op(
            "ProductSearch",
            json!({ "My": { "products": {
                "results": results,
                "totalCount": total,
                "totalPages": 1,
            }}}),
        )
        .await;
    }

    pub async fn mount_categories(&self) {
        self.mount_op(
            "GetAllCategories",
            json!({ "My": { "categories": {
                "key": "9-ROOT", "name": "All Departments", "level": 0,
                "displaySlug": "all-departments",
                "children": [
                    {
                        "key": "9-VEG", "name": "Fruit & Veg", "level": 1,
                        "displaySlug": "fruit-veg",
                        "children": [{
                            "key": "9-APPLES", "name": "Apples", "level": 2,
                            "displaySlug": "fruit-veg/apples", "children": [],
                        }],
                    },
                    {
                        "key": "9-BAKERY", "name": "Bakery", "level": 1,
                        "displaySlug": "bakery", "children": [],
                    },
                ],
            }}}),
        )
        .await;
    }

    /// Base command with the fixture's endpoints and a private config/state dir.
    pub fn cmd(&self) -> assert_cmd::Command {
        self.command(assert_cmd::Command::cargo_bin("wwnz").expect("binary built"))
    }

    /// As [`Fixture::cmd`], but running a particular copy of the binary. The
    /// update tests need this: `wwnz update` replaces the file it is running
    /// from, which must not be the one cargo just built.
    pub fn cmd_at(&self, path: &std::path::Path) -> assert_cmd::Command {
        self.command(assert_cmd::Command::new(path))
    }

    fn command(&self, mut cmd: assert_cmd::Command) -> assert_cmd::Command {
        // A developer's own environment must not reach the tests.
        for key in [
            "WWNZ_STORE_ID",
            "WWNZ_SESSION",
            "WWNZ_GUEST_TOKEN",
            "WWNZ_EMAIL",
            "WWNZ_UPDATE_API",
        ] {
            cmd.env_remove(key);
        }
        cmd.env("NO_COLOR", "1")
            .env("WWNZ_SECRET_BACKEND", "file")
            .env("WWNZ_CONFIG_DIR", self.home.path().join("config"))
            .env("WWNZ_STATE_DIR", self.home.path().join("state"))
            .env("WWNZ_ORIGIN", self.server.uri())
            .env("WWNZ_AUTH_ORIGIN", self.server.uri());
        cmd
    }

    /// As [`Fixture::cmd`], signed in.
    pub fn cmd_signed_in(&self) -> assert_cmd::Command {
        let mut cmd = self.cmd();
        cmd.env("WWNZ_SESSION", SESSION);
        cmd
    }

    /// As [`Fixture::cmd`], with a store already selected.
    pub fn cmd_with_store(&self) -> assert_cmd::Command {
        let mut cmd = self.cmd();
        cmd.env("WWNZ_STORE_ID", STORE_ID);
        cmd
    }
}

/// One store in the shape the locations query returns.
pub fn location(id: &str, name: &str, suburb: &str) -> Value {
    json!({
        "id": id,
        "storeId": id,
        "name": name,
        "description": name,
        "distance": 0.62,
        "address": {
            "lines": { "line1": "11 Kamo Road" },
            "locality": { "suburb": suburb, "city": null },
        },
    })
}

/// One product in the shape the search response returns.
///
/// The cup price is deliberately not the selling price: they are separate
/// fields, and a fixture that made them equal could not tell a renderer reading
/// the wrong one from a renderer reading the right one.
pub fn product(sku: &str, name: &str, brand: &str, dollars: f64) -> Value {
    product_priced(sku, name, brand, dollars, dollars / 2.0)
}

/// A product with an explicit cup price, for the cases that care about it.
pub fn product_priced(sku: &str, name: &str, brand: &str, dollars: f64, cup: f64) -> Value {
    json!({
        "__typename": "ProductSummary",
        "sku": sku,
        "productName": name,
        "brand": brand,
        "slug": name.to_lowercase().replace(' ', "-"),
        "storeKey": "9556",
        "imageUrl": format!("https://images.test/{sku}.jpg"),
        "categoryHierarchyNames": { "lvl1": ["Fridge & Deli"] },
        "variants": [{
            "variantKey": format!("{sku}-EA"),
            "unitOfMeasure": "EACH",
            "availabilityStatus": "IN_STOCK",
            "variantPrice": {
                "sellingPrice": dollars,
                "cupPrice": cup,
                "cupUnit": "1L",
                "isSpecial": false,
                "isClubPrice": false,
            },
        }],
    })
}

/// A product on special, which carries the price it was before.
pub fn special(sku: &str, name: &str, brand: &str, now: f64, was: f64) -> Value {
    let mut p = product(sku, name, brand, now);
    p["variants"][0]["variantPrice"]["isSpecial"] = json!(true);
    p["variants"][0]["variantPrice"]["wasPrice"] = json!(was);
    p
}

/// An ad slot, which the results list carries alongside real products.
pub fn ad_slot() -> Value {
    json!({ "__typename": "GamResultItem", "adUnitWeb": "banner" })
}

/// A cart body in the shape every cart operation answers with.
pub fn cart_body(lines: Vec<Value>) -> Value {
    let subtotal: i64 = lines
        .iter()
        .map(|l| l["lineTotal"]["afterDiscountAsCents"].as_i64().unwrap_or(0))
        .sum();
    let items: u64 = lines
        .iter()
        .map(|l| l["quantity"].as_u64().unwrap_or(0))
        .sum();
    // A pickup fee, which is what makes orderSubtotal differ from the lines.
    let fee: i64 = if lines.is_empty() { 0 } else { 500 };
    json!({
        "key": "a-cart-id",
        "totalItemQuantity": items,
        "totalUniqueProductSku": lines.len(),
        "lineItems": lines,
        "checkout": { "amountToPayAsCents": subtotal + fee },
        "pricing": {
            "orderSubtotal": {
                "afterDiscountAsCents": subtotal + fee, "discountAmountAsCents": 0,
            },
            "productSubtotal": {
                "afterDiscountAsCents": subtotal, "discountAmountAsCents": 0,
            },
        },
        "validationResult": { "isValid": true, "failedValidations": [] },
        "shoppingMode": { "pickupLocation": { "id": STORE_ID, "name": STORE_NAME } },
        "fulfilment": {
            "propositionId": "a-proposition",
            "fulfilmentProposition": {
                "storeId": "9556", "method": "pickup",
                "store": { "storeId": STORE_ID, "name": STORE_NAME },
            },
        },
    })
}

pub fn cart_line(sku: &str, name: &str, brand: &str, qty: u32, cents: i64) -> Value {
    json!({
        "sku": sku,
        "productVariantSku": format!("{sku}-EA"),
        "quantity": qty,
        "canSubstitute": false,
        "lineTotal": { "afterDiscountAsCents": cents * qty as i64, "discountAmountAsCents": 0 },
        "unitPrice": { "afterDiscountAsCents": cents },
        "product": {
            "brand": brand,
            "slug": "a-slug",
            "variants": [{ "name": name, "key": format!("{sku}-EA") }],
        },
    })
}

/// One order in the shape the orders query returns.
pub fn order(number: &str, placed: &str, cents: i64) -> Value {
    json!({
        "orderNumber": number,
        "createdDateTime": placed,
        "orderStatus": "COMPLETED",
        "fulfilmentStatus": "FULFILLED",
        "isAmendable": false,
        "total": { "afterDiscountInCents": cents },
        "fulfilments": [{
            "method": "pickup",
            "startTime": placed,
            "fulfilmentLocation": { "name": STORE_NAME },
        }],
    })
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

    pub async fn releases(&self, body: Value) {
        Mock::given(method("GET"))
            .and(path("/repos/jason-s13r/claude-playground/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    /// The release list answering with something other than a 200, which is
    /// how a rate limit arrives.
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

/// The asset names `release-build.sh` would produce for this host.
///
/// The mapping from Rust's target names to `uname`'s is the same one the client
/// makes; a test that hardcoded one platform would only ever exercise the
/// machine it was written on.
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
        .map(|arch| format!("woolworths-nz-cli-{version}-{os}-{arch}.tar.gz"))
        .collect()
}

/// A gzipped tar holding one executable file, which is what the release build
/// produces.
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
