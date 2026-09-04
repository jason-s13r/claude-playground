//! Where the storefront lives, and the URLs built against it.
//!
//! Plain fields, not resolved from the environment: this crate takes values.
//! The caller decides whether an override exists, which is also how a test
//! points the whole flow at a mock server.
//!
//! Two shapes of URL appear. The pretty ones (`/search/updategrid`, `/c/...`)
//! are storefront aliases; the rest are raw Salesforce Commerce Cloud
//! controller paths, `/on/demandware.store/Sites-twl-Site/default/<Controller>-<Action>`.
//! Both reach the same code -- the site itself uses whichever the page happened
//! to render -- so each is written here as observed rather than normalised to
//! one form.

/// The SFCC site id. Part of every controller path, and the one piece of this
/// that would differ for another Demandware storefront.
pub const SITE: &str = "Sites-twl-Site";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    /// The storefront. Everything is served from this one host.
    pub origin: String,
}

impl Default for Endpoints {
    fn default() -> Endpoints {
        Endpoints {
            origin: "https://www.thewarehouse.co.nz".into(),
        }
    }
}

impl Endpoints {
    pub fn with_origin(mut self, origin: impl Into<String>) -> Endpoints {
        self.origin = origin.into().trim_end_matches('/').to_string();
        self
    }

    /// A raw SFCC controller, `Stores-FindStores` and the like.
    pub fn controller(&self, action: &str) -> String {
        format!(
            "{}/on/demandware.store/{SITE}/default/{action}",
            self.origin
        )
    }

    /// The paging endpoint. Serves both search and browse: it takes `q` or
    /// `cgid`, and answers with an HTML fragment of product tiles.
    pub fn updategrid(&self) -> String {
        format!("{}/search/updategrid", self.origin)
    }

    /// The full search page, which is the only thing that reveals a keyword
    /// redirecting into a category.
    pub fn search(&self) -> String {
        format!("{}/search", self.origin)
    }

    pub fn suggestions(&self) -> String {
        format!("{}/search/suggestions", self.origin)
    }

    /// A category landing page, from the `/c/...` path a category record
    /// carries. Takes the path so a caller cannot accidentally build one from
    /// a category *id* -- the two differ (`homegarden` vs
    /// `/c/home-garden-appliances`).
    pub fn category_page(&self, path: &str) -> String {
        format!("{}/{}", self.origin, path.trim_start_matches('/'))
    }

    /// A product detail page. The slug is decoration -- SFCC resolves on the
    /// id alone -- so a caller holding only an id can pass `"p"`.
    pub fn product_page(&self, slug: &str, pid: &str) -> String {
        format!("{}/p/{}/{}.html", self.origin, slug.trim_matches('/'), pid)
    }

    pub fn variation(&self) -> String {
        format!("{}/products/variation", self.origin)
    }

    /// Per-store stock, narrowed to a region.
    ///
    /// The storefront alias, not the `Stores-PDPStoreAvailability` controller
    /// it routes to. The two are not interchangeable: the raw controller path
    /// answers 403 `Cross-Origin Request Blocked`, while this one -- the URL the
    /// site's own page calls -- is served.
    pub fn stores_region(&self) -> String {
        format!("{}/products/stores/region", self.origin)
    }

    pub fn minicart(&self) -> String {
        format!("{}/cart/minicart", self.origin)
    }

    pub fn cart_page(&self) -> String {
        format!("{}/cart", self.origin)
    }

    pub fn wishlist_page(&self) -> String {
        format!("{}/wishlist", self.origin)
    }

    pub fn login(&self) -> String {
        format!("{}/account/submit-login", self.origin)
    }

    pub fn login_page(&self) -> String {
        format!("{}/login", self.origin)
    }

    pub fn account_page(&self) -> String {
        format!("{}/account", self.origin)
    }

    /// A relative action URL scraped from a page, made absolute.
    ///
    /// Every `verify`-bearing URL arrives relative, and the token is already
    /// percent-encoded in the markup -- so this concatenates rather than
    /// re-parsing, which would double-encode it.
    pub fn absolute(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else {
            format!("{}/{}", self.origin, path.trim_start_matches('/'))
        }
    }
}

/// Percent-encode one query component.
///
/// Hand-rolled rather than taken from `url` because this crate must never
/// re-parse a URL it was given: the `verify` tokens arrive already encoded, and
/// round-tripping one through a URL type re-encodes the `%2F` in its signature
/// and the server stops recognising it. Encoding only the pieces it builds
/// itself is the rule that keeps that impossible.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `a=1&b=2` from pairs, in the order given.
///
/// Order matters here: SFCC reads refinements as numbered pairs, so `prefn1`
/// has to precede the `prefv1` it belongs to.
pub fn query_string(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_string_encodes_values_and_keeps_the_order_it_was_given() {
        let params = [
            ("q".to_string(), "blue shirt".to_string()),
            ("prefn1".to_string(), "islandAvailability".to_string()),
            ("prefv1".to_string(), "northIsland".to_string()),
        ]
        .to_vec();
        assert_eq!(
            query_string(&params),
            "q=blue%20shirt&prefn1=islandAvailability&prefv1=northIsland"
        );
    }

    #[test]
    fn overrides_trim_a_trailing_slash() {
        let e = Endpoints::default().with_origin("http://127.0.0.1:9/");
        assert_eq!(e.updategrid(), "http://127.0.0.1:9/search/updategrid");
        assert_eq!(
            e.controller("Stores-FindStores"),
            "http://127.0.0.1:9/on/demandware.store/Sites-twl-Site/default/Stores-FindStores"
        );
    }

    #[test]
    fn a_scraped_action_url_keeps_its_encoded_verify_token() {
        // Re-parsing would turn %2F back into a slash and the HMAC would no
        // longer match what the server signed.
        let e = Endpoints::default();
        let raw = "/cart/add-product?pid=R1&verify=1788496536-sRRVB4dd%2F4n2%3D";
        assert_eq!(
            e.absolute(raw),
            "https://www.thewarehouse.co.nz/cart/add-product?pid=R1&verify=1788496536-sRRVB4dd%2F4n2%3D"
        );
    }

    #[test]
    fn an_already_absolute_url_is_left_alone() {
        let e = Endpoints::default();
        assert_eq!(
            e.absolute("https://elsewhere.test/x"),
            "https://elsewhere.test/x"
        );
    }
}
