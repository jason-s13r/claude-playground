//! Reading The Warehouse's HTML.
//!
//! **The only module in this crate that parses markup**, which is the point of
//! it: when the site is redesigned, exactly one file fails to compile against
//! reality and exactly one file's tests go red.
//!
//! Listings are HTML because there is no product API to ask instead. That is
//! more tractable than it sounds, because every tile carries a
//! `data-gtm-product` attribute holding a JSON object -- name, id, brand, EAN,
//! price, category -- put there for the site's own analytics. Reading that is
//! one attribute lookup and a `serde_json` parse rather than a walk over
//! presentational CSS classes, so a restyle does not break it and a genuine
//! change surfaces as a parse error rather than as silently wrong prices.
//!
//! What the attribute does *not* carry is fetched from the surrounding markup:
//! the link, the image, the displayed price and the stock status. Each is
//! optional, so a tile that has moved on degrades to a missing field.

use scraper::{Html, Selector};

use crate::domain::{Availability, Price, Product, StoreStock};

/// Compile a selector once. A bad selector here is a programming error, not
/// something the site can cause, so panicking names the bug immediately rather
/// than turning it into an empty result at runtime.
macro_rules! sel {
    ($name:ident, $css:literal) => {
        static $name: std::sync::LazyLock<Selector> =
            std::sync::LazyLock::new(|| Selector::parse($css).expect(concat!("selector: ", $css)));
    };
}

sel!(TILE, "div.product-tile");
sel!(TILE_LINK, "a.link, section.product-tile-image a[href]");
sel!(TILE_IMAGE, "img.tile-image");
sel!(TILE_PRICE, "div.price");
sel!(TILE_WAS, "div.strike-through, span.strike-through, del");
sel!(TILE_STOCK, "div.availability-stock-status");
sel!(
    TILE_RATING,
    "div.ratings [data-rating], div.ratings .rating-value"
);
sel!(HEADER_COUNT, "div.filter-header p, div.header-bar p");
sel!(GRID_FOOTER, "div.grid-footer");
sel!(LD_JSON, r#"script[type="application/ld+json"]"#);
// Five attributes, because the site puts action URLs on whichever suits the
// element: `href` on links, `url` on its custom elements, `value` on hidden
// inputs, and `data-url`/`data-href` on the radio inputs and `<option>`s that
// drive variations and the region picker.
sel!(
    ACTION_URL,
    "[href], [url], [data-url], [data-href], input.add-to-cart-url"
);
sel!(STORE_PANEL, "div.store.panel");
sel!(STORE_TITLE, "h6.title, .store-details-title h6");
sel!(STORE_AVAIL, "span.store-availability");
sel!(STORE_TOGGLE, "[data-target]");
sel!(
    REGION_OPTION,
    "[data-href], a[href*='/products/stores/region']"
);
sel!(
    SUGGEST_PRODUCT,
    "a.suggestion-product, .product-suggestions a[href*='/p/']"
);

/// The tracking payload on every tile.
///
/// Read as a loose map rather than deserialised into a struct, and that is not
/// laziness. The object carries about thirty keys, it is written for the site's
/// own analytics, and **its field types are not stable**: `productRating`
/// arrives as `5` on one tile, `"4.6"` on another and `"na"` on a third, and
/// the variation group is called `variationGroupId` on some pages and
/// `variationProductId` on others. A struct with a declared type per field
/// fails the *whole* payload when any one of them varies -- losing the name,
/// the brand and the barcode because of the rating -- which is exactly the
/// failure this crate says it will not have.
struct Gtm(serde_json::Map<String, serde_json::Value>);

impl Gtm {
    fn read(raw: &str) -> Gtm {
        Gtm(serde_json::from_str(raw).unwrap_or_default())
    }

    /// A field as text, whatever it was written as.
    ///
    /// The site writes `"na"` where a value is absent, so that literal and an
    /// empty string both mean nothing.
    fn str(&self, key: &str) -> Option<String> {
        let text = match self.0.get(key)? {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => return None,
        };
        let text = text.trim();
        (!text.is_empty() && text != "na" && text != "undefined").then(|| text.to_string())
    }

    /// The first of several names for one thing, for a field the site has
    /// renamed between templates.
    fn any(&self, keys: &[&str]) -> Option<String> {
        keys.iter().find_map(|k| self.str(k))
    }

    fn number(&self, key: &str) -> Option<f64> {
        self.str(key)?.parse().ok()
    }

    fn flag(&self, key: &str) -> Option<bool> {
        match self.0.get(key)? {
            serde_json::Value::Bool(b) => Some(*b),
            serde_json::Value::String(s) => s.parse().ok(),
            _ => None,
        }
    }
}

/// Every product tile in a listing page or an `updategrid` fragment.
///
/// One parser for both: the fragment is the same markup as the grid inside a
/// full page, which is why page one and page N do not need separate handling.
pub fn tiles(html: &str) -> Vec<Product> {
    let doc = Html::parse_fragment(html);
    doc.select(&TILE)
        .filter_map(|tile| {
            let gtm = Gtm::read(tile.attr("data-gtm-product").unwrap_or("{}"));

            let href = tile
                .select(&TILE_LINK)
                .find_map(|a| a.attr("href"))
                .map(str::to_string);

            // Four sources for the id, in order of how well they address the
            // thing: the link is what the site itself would follow, the
            // variation group is what the tracking payload says the link points
            // at, `data-pid` is on the tile wrapper, and the master id is the
            // last resort -- it may address a family rather than a buyable item.
            let master = gtm.str("id");
            let id = href
                .as_deref()
                .and_then(pid_from_path)
                .or_else(|| gtm.any(&["variationGroupId", "variationProductId"]))
                .or_else(|| tile.attr("data-pid").map(str::to_string))
                .or_else(|| master.clone())?;

            let name = gtm.str("name").or_else(|| {
                tile.select(&TILE_LINK)
                    .map(|a| a.text().collect::<String>().trim().to_string())
                    .find(|t| !t.is_empty())
            })?;

            // The displayed price is preferred over the tracked one: it is what a
            // person would see, and it carries the currency symbol.
            let shown = tile
                .select(&TILE_PRICE)
                .map(|e| e.text().collect::<String>().trim().to_string())
                .find(|t| !t.is_empty());
            let price = match shown {
                Some(text) => Price::from_display(&text),
                None => Price {
                    value: gtm.number("price"),
                    formatted: None,
                    currency: None,
                },
            };
            let was_price = tile
                .select(&TILE_WAS)
                .map(|e| e.text().collect::<String>().trim().to_string())
                .find(|t| !t.is_empty())
                .map(|t| Price::from_display(&t));

            Some(Product {
                id,
                master_id: master,
                name,
                brand: gtm.str("brand"),
                ean: gtm.str("productEAN"),
                price,
                was_price,
                rating: gtm.number("productRating").or_else(|| {
                    tile.select(&TILE_RATING)
                        .find_map(|e| e.attr("data-rating").and_then(|v| v.parse().ok()))
                }),
                category: gtm.str("category"),
                url: href,
                image: tile
                    .select(&TILE_IMAGE)
                    .find_map(|i| i.attr("src"))
                    .map(str::to_string),
                availability: tile_availability(&tile),
                marketplace: gtm.flag("marketplaceProduct").unwrap_or(false),
            })
        })
        .collect()
}

/// The tile's own stock marker, which is structured rather than prose:
/// `data-stock-status="IN_STOCK" data-orderable="true"`.
fn tile_availability(tile: &scraper::ElementRef<'_>) -> Availability {
    let Some(node) = tile.select(&TILE_STOCK).next() else {
        return Availability::default();
    };
    let status = node.attr("data-stock-status").map(str::to_string);
    let orderable = node.attr("data-orderable").map(|v| v == "true");
    let in_store = status.as_deref().map(|s| s == "FIND_IN_STORE");
    Availability {
        online: match (orderable, in_store) {
            // "Find in store" is orderable, but not online -- the tile's one
            // boolean has to be read against the status to say which channel.
            (Some(true), Some(true)) => Some(false),
            (o, _) => o,
        },
        in_store: match (orderable, in_store) {
            (Some(true), Some(true)) => Some(true),
            (Some(false), _) => Some(false),
            _ => None,
        },
        label: node
            .text()
            .collect::<String>()
            .trim()
            .to_string()
            .into_option(),
        status,
    }
}

/// `RM110164727-1M` out of `/p/young-original-nylon-swim-shorts/RM110164727-1M.html`.
pub fn pid_from_path(path: &str) -> Option<String> {
    // Query first, then the extension: a tile links with a variation already
    // chosen (`...RM1-2M.html?dwvar_RM1-2M_color=RED`), so stripping `.html`
    // from the raw path would not match.
    let path = path.split(['?', '#']).next()?;
    let file = path.rsplit('/').next()?;
    let pid = file.strip_suffix(".html")?;
    (!pid.is_empty()).then(|| pid.to_string())
}

/// How many products the whole listing has, from the `65 - 96 of 3,122
/// products` line the grid is headed with.
///
/// This is the only place the total appears -- the fragment carries no count of
/// its own -- so paging past the end is detected by an empty page as well.
pub fn listing_total(html: &str) -> Option<u32> {
    let doc = Html::parse_fragment(html);
    doc.select(&HEADER_COUNT).find_map(|p| {
        let text = p.text().collect::<String>();
        let (_, after) = text.split_once(" of ")?;
        let digits: String = after.chars().filter(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

/// The sort rules this listing offers, as `(id, display name)`.
///
/// Read from the page rather than hard-coded so a rule The Warehouse adds is
/// offered without a release, and one it removes stops being suggested.
pub fn sort_options(html: &str) -> Vec<(String, String)> {
    #[derive(serde::Deserialize)]
    struct Options {
        #[serde(default)]
        options: Vec<Option_>,
    }
    #[derive(serde::Deserialize)]
    struct Option_ {
        id: String,
        #[serde(rename = "displayName")]
        display_name: String,
    }

    let doc = Html::parse_fragment(html);
    doc.select(&GRID_FOOTER)
        .filter_map(|f| f.attr("data-sort-options"))
        .filter_map(|raw| serde_json::from_str::<Options>(raw).ok())
        .flat_map(|o| o.options)
        .map(|o| {
            // The site suffixes the default rule's id in the sort menu but not
            // in the URL it builds, so `srule` would be rejected as written.
            let id = o.id.strip_suffix("-option").unwrap_or(&o.id).to_string();
            (id, o.display_name)
        })
        .collect()
}

/// The `verify`-bearing action URLs a product page was rendered with.
///
/// These are the whole reason a write is a two-step. Each is a server-minted
/// HMAC over the action and a timestamp, embedded in the page, and no endpoint
/// that takes one can be called without first fetching the page that carries
/// it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Actions {
    pub add_to_cart: Option<String>,
    pub add_to_wishlist: Option<String>,
    pub store_stock: Option<String>,
    pub shipping: Option<String>,
    pub set_region: Option<String>,
    /// The variation endpoint with no value chosen, which is the one to build
    /// a `dwvar_` selection on top of.
    pub variation: Option<String>,
}

impl Actions {
    pub fn is_empty(&self) -> bool {
        *self == Actions::default()
    }
}

/// Scrape a product page for the actions it authorises.
///
/// Attribute-driven rather than position-driven: the add-to-cart URL is on a
/// hidden input, the wishlist one on a custom element, and both move around the
/// page between templates. What does not move is that the URL names its own
/// endpoint.
pub fn actions(html: &str) -> Actions {
    let doc = Html::parse_fragment(html);
    let mut found = Actions::default();

    for node in doc.select(&ACTION_URL) {
        for attr in ["href", "url", "data-url", "data-href", "value"] {
            let Some(raw) = node.attr(attr) else { continue };
            if !raw.contains("verify=") {
                continue;
            }
            let url = raw.to_string();
            let slot = if raw.contains("/cart/add-product") {
                &mut found.add_to_cart
            } else if raw.contains("/wishlist-add-product") {
                &mut found.add_to_wishlist
            } else if raw.contains("/products/stores") {
                &mut found.store_stock
            } else if raw.contains("/products/shipping") {
                &mut found.shipping
            } else if raw.contains("/products/set-region") {
                &mut found.set_region
            } else if raw.contains("/products/variation") {
                // A page carries one of these per selectable value. The bare
                // one -- no `dwvar_` chosen -- is the base to build on; anything
                // else would arrive with a value already selected.
                if raw.contains("dwvar_") {
                    continue;
                }
                &mut found.variation
            } else {
                continue;
            };
            // First wins. A page repeats the same action in several templates
            // and they carry the same token.
            slot.get_or_insert(url);
        }
    }
    found
}

/// The schema.org `Product` block a detail page carries.
///
/// A second, independent description of the same product -- so it is a cheap
/// cross-check on the scraped half, and the only place `gtin13` and the
/// canonical description appear.
#[derive(Debug, Default, serde::Deserialize)]
pub struct LdProduct {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub brand: Option<LdBrand>,
    #[serde(default)]
    pub offers: Option<LdOffer>,
    #[serde(default)]
    pub gtin13: Vec<String>,
    #[serde(default)]
    pub image: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct LdBrand {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct LdOffer {
    #[serde(default)]
    pub price: Option<String>,
    #[serde(default, rename = "priceCurrency")]
    pub currency: Option<String>,
    /// A schema.org URL: `http://schema.org/InStock`.
    #[serde(default)]
    pub availability: Option<String>,
}

/// The first `ld+json` block that is a Product. A page carries several --
/// breadcrumbs and the organisation as well -- so the type has to be checked
/// rather than the first one taken.
pub fn json_ld(html: &str) -> Option<LdProduct> {
    let doc = Html::parse_fragment(html);
    doc.select(&LD_JSON).find_map(|s| {
        let raw = s.text().collect::<String>();
        let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
        (value.get("@type")?.as_str()? == "Product").then_some(())?;
        serde_json::from_value(value).ok()
    })
}

/// Per-store stock, out of the rendered modal the stock endpoint answers with.
///
/// This is the ugliest parse in the crate and it is not by choice: the endpoint
/// returns HTML inside a JSON field, so "in stock" arrives as a CSS class on a
/// span. The class is at least a machine value rather than prose, which is what
/// makes `in_stock` answerable; the visible text is kept either way so an
/// unfamiliar state still prints something true.
pub fn store_stock(html: &str) -> Vec<StoreStock> {
    let doc = Html::parse_fragment(html);
    doc.select(&STORE_PANEL)
        .filter_map(|panel| {
            let store_name = panel
                .select(&STORE_TITLE)
                .next()?
                .text()
                .collect::<String>()
                .trim()
                .to_string();
            let avail = panel.select(&STORE_AVAIL).next();
            let status = avail.and_then(|a| {
                a.value()
                    .classes()
                    .find_map(|c| c.strip_prefix("store-availability__"))
                    .map(str::to_string)
            });
            Some(StoreStock {
                store_id: panel
                    .select(&STORE_TOGGLE)
                    .find_map(|t| t.attr("data-target"))
                    .and_then(|t| t.rsplit('-').next().map(str::to_string)),
                store_name,
                label: avail
                    .map(|a| a.text().collect::<String>().trim().to_string())
                    .and_then(StringExt::into_option),
                in_stock: status.as_deref().map(|s| s == "IN_STOCK"),
            })
        })
        .collect()
}

/// A description as prose.
///
/// `longDescription` is a fragment of HTML -- headings, a `<ul>` of features --
/// arriving inside a JSON field. Printing it raw puts tags in a terminal;
/// stripping tags blindly runs the list items together into one sentence. So
/// the block elements become line breaks and list items become bullets, which
/// is the least that keeps it readable.
pub fn plain_text(html: &str) -> String {
    let doc = Html::parse_fragment(html);
    let mut out = String::new();
    write_text(doc.root_element().first_child(), &mut out);

    // Collapse the runs of blank lines the block breaks leave behind.
    let mut lines: Vec<&str> = Vec::new();
    for line in out.lines() {
        let line = line.trim_end();
        if line.is_empty() && lines.last().is_some_and(|l: &&str| l.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

fn write_text(node: Option<ego_tree::NodeRef<'_, scraper::Node>>, out: &mut String) {
    let mut current = node;
    while let Some(node) = current {
        match node.value() {
            scraper::Node::Text(text) => {
                // Newlines inside a paragraph are formatting in the source, not
                // meaning; the block elements are what break lines here.
                out.push_str(&text.replace('\n', " "));
            }
            scraper::Node::Element(element) => {
                let name = element.name();
                if matches!(name, "script" | "style") {
                    current = node.next_sibling();
                    continue;
                }
                if name == "li" {
                    out.push_str("\n  - ");
                } else if matches!(
                    name,
                    "p" | "div" | "br" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol"
                ) {
                    out.push('\n');
                }
                write_text(node.first_child(), out);
                if !matches!(name, "br" | "li") {
                    out.push('\n');
                }
            }
            _ => {}
        }
        current = node.next_sibling();
    }
}

/// The per-region stock URLs a stock modal was rendered with.
///
/// One pre-signed URL per region, keyed by region code. These are the reason
/// narrowing stock to a region is a *third* request rather than a parameter on
/// the second: each carries its own `verify` token, minted when the modal was
/// rendered, and a token from the product page will not do -- the server
/// answers a built URL with `Cross-Origin Request Blocked` however well formed
/// it looks.
pub fn region_links(html: &str) -> std::collections::BTreeMap<String, String> {
    let doc = Html::parse_fragment(html);
    doc.select(&REGION_OPTION)
        .filter_map(|node| {
            // The modal renders them as `<option value="NZ-NTL" data-href="...">`
            // rather than as links, so the URL is on `data-href` and the region
            // code is the option's value.
            let raw = node
                .attr("data-href")
                .or_else(|| node.attr("href"))
                .filter(|v| v.contains("/products/stores/region"))?;
            let region = node.attr("value").map(str::to_string).or_else(|| {
                raw.split(['?', '&'])
                    .find_map(|pair| pair.strip_prefix("region="))
                    .map(str::to_string)
            })?;
            (!region.is_empty()).then(|| (region, raw.to_string()))
        })
        .collect()
}

/// Products the typeahead offered, as `(pid, name)`.
pub fn suggestions(html: &str) -> Vec<(String, String)> {
    let doc = Html::parse_fragment(html);
    doc.select(&SUGGEST_PRODUCT)
        .filter_map(|a| {
            let pid = pid_from_path(a.attr("href")?)?;
            let name = a.text().collect::<String>().trim().to_string();
            (!name.is_empty()).then_some((pid, name))
        })
        .collect()
}

trait StringExt {
    fn into_option(self) -> Option<String>;
}

impl StringExt for String {
    fn into_option(self) -> Option<String> {
        (!self.is_empty()).then_some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TILE_HTML: &str = r##"
<div class="product-tile" data-gtm-product="{&quot;name&quot;:&quot;Plain Cotton Tee&quot;,&quot;id&quot;:&quot;RM100000001&quot;,&quot;brand&quot;:&quot;Example Brand&quot;,&quot;productEAN&quot;:&quot;9400000000001&quot;,&quot;variationGroupId&quot;:&quot;RM100000001-1M&quot;,&quot;productRating&quot;:&quot;4.6&quot;,&quot;marketplaceProduct&quot;:false,&quot;category&quot;:&quot;clothing/clothing-tops&quot;,&quot;price&quot;:&quot;12.00&quot;}">
  <section class="product-tile-image">
    <a href="/p/plain-cotton-tee/RM100000001-1M.html"><img class="tile-image" src="https://example.test/tee.jpg" /></a>
    <div class="availability-stock-status" data-stock-status="IN_STOCK" data-orderable="true"></div>
  </section>
  <section class="product-tile-details">
    <div class="price">$12.00</div>
    <a class="link" href="/p/plain-cotton-tee/RM100000001-1M.html">Plain Cotton Tee</a>
  </section>
</div>"##;

    #[test]
    fn a_tile_is_read_from_its_tracking_payload_and_its_markup() {
        let products = tiles(TILE_HTML);
        assert_eq!(products.len(), 1);
        let p = &products[0];
        // The link, not the tracking id: the master would 404 the cart.
        assert_eq!(p.id, "RM100000001-1M");
        assert_eq!(p.master_id.as_deref(), Some("RM100000001"));
        assert!(p.is_variant());
        assert_eq!(p.name, "Plain Cotton Tee");
        assert_eq!(p.brand.as_deref(), Some("Example Brand"));
        assert_eq!(p.ean.as_deref(), Some("9400000000001"));
        assert_eq!(p.price.value, Some(12.0));
        assert_eq!(p.price.formatted.as_deref(), Some("$12.00"));
        assert_eq!(p.rating, Some(4.6));
        assert_eq!(p.image.as_deref(), Some("https://example.test/tee.jpg"));
        assert_eq!(p.availability.online, Some(true));
        assert!(!p.marketplace);
    }

    #[test]
    fn a_tile_with_no_tracking_payload_still_yields_a_product() {
        // The stated rule for this crate: a field The Warehouse changes should
        // cost a column, not the command.
        let html = r#"<div class="product-tile">
            <a class="link" href="/p/slug/R9999999.html">Something</a>
            <div class="price">$4.50</div>
        </div>"#;
        let products = tiles(html);
        assert_eq!(products.len(), 1);
        assert_eq!(products[0].id, "R9999999");
        assert_eq!(products[0].name, "Something");
        assert_eq!(products[0].price.value, Some(4.50));
        assert_eq!(products[0].brand, None);
        assert_eq!(products[0].availability.orderable(), None);
    }

    #[test]
    fn a_tile_with_neither_a_link_nor_a_payload_is_skipped_not_fatal() {
        let html = r#"<div class="product-tile"><div class="price">$1.00</div></div>"#;
        assert!(tiles(html).is_empty());
    }

    #[test]
    fn one_oddly_typed_field_does_not_cost_the_whole_payload() {
        // Observed live: `productRating` is a JSON number here and a string in
        // the captures. A struct with a declared type for it failed the entire
        // object, and the tile silently lost its brand, barcode and category --
        // which is the failure mode this crate exists to avoid.
        let html = r##"<div class="product-tile" data-gtm-product="{&quot;name&quot;:&quot;A Thing&quot;,&quot;id&quot;:&quot;R1&quot;,&quot;brand&quot;:&quot;Example Brand&quot;,&quot;productEAN&quot;:&quot;9400000000004&quot;,&quot;productRating&quot;:5,&quot;category&quot;:&quot;toysbaby&quot;}">
            <a class="link" href="/p/a-thing/R1.html">A Thing</a></div>"##;
        let p = &tiles(html)[0];
        assert_eq!(p.rating, Some(5.0), "a numeric rating reads as a number");
        assert_eq!(p.brand.as_deref(), Some("Example Brand"));
        assert_eq!(p.ean.as_deref(), Some("9400000000004"));
        assert_eq!(p.category.as_deref(), Some("toysbaby"));
    }

    #[test]
    fn both_names_for_the_variation_group_are_accepted() {
        // `variationGroupId` in one capture, `variationProductId` live.
        for key in ["variationGroupId", "variationProductId"] {
            let html = format!(
                r#"<div class="product-tile" data-gtm-product='{{"name":"A Thing","id":"RM1","{key}":"RM1-2M"}}'>
                   <span>no link</span></div>"#
            );
            assert_eq!(tiles(&html)[0].id, "RM1-2M", "{key}");
        }
    }

    #[test]
    fn a_tile_with_no_link_falls_back_to_the_pid_on_the_wrapper() {
        let html = r#"<div class="product-tile" data-pid="R7"
            data-gtm-product='{"name":"A Thing"}'><span>no link</span></div>"#;
        assert_eq!(tiles(html)[0].id, "R7");
    }

    #[test]
    fn the_sites_na_placeholder_reads_as_absent() {
        let html = r##"<div class="product-tile" data-gtm-product="{&quot;name&quot;:&quot;X&quot;,&quot;id&quot;:&quot;R1&quot;,&quot;brand&quot;:&quot;na&quot;,&quot;productRating&quot;:&quot;na&quot;,&quot;variationGroupId&quot;:&quot;na&quot;}">
            <a class="link" href="/p/x/R1.html">X</a></div>"##;
        let p = &tiles(html)[0];
        assert_eq!(p.brand, None, "\"na\" is the site writing nothing");
        assert_eq!(p.rating, None);
        assert_eq!(p.id, "R1");
    }

    #[test]
    fn find_in_store_stock_reads_as_a_shop_not_as_online() {
        let html = r#"<div class="product-tile">
            <a class="link" href="/p/x/R2.html">X</a>
            <div class="availability-stock-status" data-stock-status="FIND_IN_STORE" data-orderable="true">In-store only</div>
        </div>"#;
        let a = &tiles(html)[0].availability;
        assert_eq!(a.online, Some(false));
        assert_eq!(a.in_store, Some(true));
        assert_eq!(a.summary(), "in store");
        assert_eq!(a.label.as_deref(), Some("In-store only"));
    }

    #[test]
    fn the_total_comes_out_of_the_grid_header() {
        let html = r#"<div class="filter-header"><div class="header-bar">
            <p class="mb-0">65 &ndash; 96 of 3,122 products</p></div></div>"#;
        assert_eq!(listing_total(html), Some(3122));
        assert_eq!(listing_total("<div></div>"), None);
    }

    #[test]
    fn a_pid_is_taken_off_the_end_of_a_product_path() {
        assert_eq!(
            pid_from_path("/p/a-slug/R123.html").as_deref(),
            Some("R123")
        );
        assert_eq!(
            pid_from_path("/p/a-slug/RM1-2M.html?dwvar_x=y").as_deref(),
            Some("RM1-2M")
        );
        assert_eq!(pid_from_path("/c/toys-baby"), None);
    }

    #[test]
    fn the_default_sort_rule_loses_the_suffix_the_menu_gives_it() {
        // `default-navigation-option` is a DOM id; `srule` wants
        // `default-navigation` and rejects the other.
        let html = r##"<div class="grid-footer" data-sort-options="{&quot;options&quot;:[{&quot;displayName&quot;:&quot;Best Match&quot;,&quot;id&quot;:&quot;default-navigation-option&quot;},{&quot;displayName&quot;:&quot;Price Low To High&quot;,&quot;id&quot;:&quot;price-low-to-high&quot;}]}"></div>"##;
        let sorts = sort_options(html);
        assert_eq!(sorts[0].0, "default-navigation");
        assert_eq!(sorts[1].0, "price-low-to-high");
    }

    #[test]
    fn every_verify_bearing_action_on_a_page_is_found_by_what_it_points_at() {
        let html = r##"
        <input type="hidden" class="add-to-cart-url" value="/cart/add-product?pid=R1&amp;verify=1-aaa" />
        <gep-add-to-wishlist url="/wishlist-add-product?pid=R1&amp;verify=1-bbb"></gep-add-to-wishlist>
        <a href="/products/stores?pid=R1&amp;verify=1-ccc">stock</a>
        <a href="/products/variation?dwvar_R1_color=RED&amp;pid=R1&amp;verify=1-ddd">red</a>
        <a href="/products/variation?pid=R1&amp;verify=1-eee">base</a>
        <a href="/somewhere/else">not an action</a>"##;
        let a = actions(html);
        assert_eq!(
            a.add_to_cart.as_deref(),
            Some("/cart/add-product?pid=R1&verify=1-aaa")
        );
        assert_eq!(
            a.add_to_wishlist.as_deref(),
            Some("/wishlist-add-product?pid=R1&verify=1-bbb")
        );
        assert_eq!(
            a.store_stock.as_deref(),
            Some("/products/stores?pid=R1&verify=1-ccc")
        );
        // The bare variation URL, not one of the per-value ones.
        assert_eq!(
            a.variation.as_deref(),
            Some("/products/variation?pid=R1&verify=1-eee")
        );
        assert_eq!(a.shipping, None);
    }

    #[test]
    fn a_variation_url_is_found_on_the_radio_input_that_carries_it() {
        // The site does not use links for these: each colour is a radio input
        // with the signed URL on `data-url`, and the bare one -- no `dwvar_`
        // chosen -- is the base to build a selection on.
        let html = r##"
        <input type="radio" name="color" value="BGE"
               data-url="/products/variation?dwvar_R1_color=BGE&amp;pid=R1&amp;verify=1-a" />
        <input type="radio" name="color" value="BLU"
               data-url="/products/variation?pid=R1&amp;quantity=1&amp;verify=1-b" />"##;
        assert_eq!(
            actions(html).variation.as_deref(),
            Some("/products/variation?pid=R1&quantity=1&verify=1-b")
        );
    }

    #[test]
    fn a_page_carrying_no_actions_says_so_rather_than_looking_populated() {
        assert!(actions("<div>nothing here</div>").is_empty());
    }

    #[test]
    fn the_product_block_is_picked_out_of_several_ld_json_blocks() {
        let html = r#"
        <script type="application/ld+json">{"@type":"BreadcrumbList","itemListElement":[]}</script>
        <script type="application/ld+json">{"@context":"http://schema.org/","@type":"Product","name":"Example Milk 3L","sku":"R1","brand":{"name":"Example"},"gtin13":["9400000000002"],"offers":{"@type":"Offer","price":"8.99","priceCurrency":"NZD","availability":"http://schema.org/InStock"}}</script>"#;
        let ld = json_ld(html).expect("a Product block");
        assert_eq!(ld.name.as_deref(), Some("Example Milk 3L"));
        assert_eq!(ld.sku.as_deref(), Some("R1"));
        assert_eq!(ld.gtin13, vec!["9400000000002"]);
        assert_eq!(ld.offers.unwrap().price.as_deref(), Some("8.99"));
    }

    #[test]
    fn a_html_description_becomes_readable_prose() {
        // Blocks separate with a blank line, list items become bullets, and the
        // runs of whitespace the source is indented with collapse.
        let html = "<p>A comfortable tee.</p><h3>Features</h3>\
                    <ul><li>100% cotton</li><li>Round neck</li></ul>";
        assert_eq!(
            plain_text(html),
            "A comfortable tee.\n\nFeatures\n\n  - 100% cotton\n  - Round neck"
        );
    }

    #[test]
    fn a_description_that_is_already_plain_is_left_alone() {
        assert_eq!(plain_text("Just a sentence."), "Just a sentence.");
        assert_eq!(plain_text(""), "");
    }

    #[test]
    fn the_stock_modal_carries_one_signed_url_per_region() {
        // As the modal actually renders them: a `<select>` of options, each
        // carrying its own signed URL on `data-href`.
        let html = r##"<select>
          <option value="">Select a region</option>
          <option value="NZ-AUK" data-href="/products/stores/region?productId=R1&amp;region=NZ-AUK&amp;verify=1-aaa">Auckland</option>
          <option value="NZ-CAN" data-href="/products/stores/region?productId=R1&amp;region=NZ-CAN&amp;verify=1-bbb">Canterbury</option>
        </select>
        <a href="/products/stores?pid=R1&amp;verify=1-ccc">all</a>"##;
        let links = region_links(html);
        assert_eq!(links.len(), 2);
        assert_eq!(
            links["NZ-CAN"],
            "/products/stores/region?productId=R1&region=NZ-CAN&verify=1-bbb"
        );
    }

    #[test]
    fn per_store_stock_is_read_from_the_class_and_the_words_are_kept_too() {
        let html = r##"
        <div class="store panel">
          <div class="store-toggle" data-target="#c-full-store-details-116">
            <div class="store-details-title"><h6 class="title">Example Town</h6></div>
            <span class="store-availability store-availability__IN_STOCK">In stock</span>
          </div>
        </div>
        <div class="store panel">
          <div class="store-toggle" data-target="#c-full-store-details-204">
            <div class="store-details-title"><h6 class="title">Other Town</h6></div>
            <span class="store-availability store-availability__NOT_AVAILABLE">Not available</span>
          </div>
        </div>"##;
        let stock = store_stock(html);
        assert_eq!(stock.len(), 2);
        assert_eq!(stock[0].store_id.as_deref(), Some("116"));
        assert_eq!(stock[0].store_name, "Example Town");
        assert_eq!(stock[0].in_stock, Some(true));
        assert_eq!(stock[1].in_stock, Some(false));
        assert_eq!(stock[1].label.as_deref(), Some("Not available"));
    }
}
