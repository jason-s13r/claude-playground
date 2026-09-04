//! Product detail, and the page tokens everything else is bought with.
//!
//! [`Pdp`] is how this crate makes the two-step explicit. Cart, wishlist, stock
//! and variation endpoints all need a `verify` token minted into the product
//! page and stamped with the time it was minted, so none of them can be called
//! cold. Rather than hiding that behind a method that silently fetches a page
//! first, the token-bearing operations take a `Pdp` -- so the fetch is
//! something the caller did, and the expiry is something it can see.

use crate::domain::ProductDetail;
use crate::error::{Error, Result};
use crate::extract::{self, Actions};

/// A product page that has been read, and what it authorises.
///
/// Short-lived by nature: the tokens carry a timestamp and the server refuses
/// them once they age out. [`Pdp::minted_at`] is that timestamp, read back out
/// of the token, so a caller can tell a stale page from a rejected one.
#[derive(Clone, Debug)]
pub struct Pdp {
    /// The id the page was fetched for, which is what its tokens are signed
    /// against. Using them with a different pid does not work.
    pub pid: String,
    pub detail: ProductDetail,
    pub actions: Actions,
}

impl Pdp {
    /// Read a product page.
    ///
    /// The detail is assembled from two independent descriptions the page
    /// carries -- the schema.org block and the product tile -- because neither
    /// is complete: the tile has stock and the category, the JSON-LD has the
    /// description, the SKU and the barcode.
    pub fn parse(pid: &str, html: &str) -> Result<Pdp> {
        let actions = extract::actions(html);
        let ld = extract::json_ld(html);
        let tile = extract::tiles(html)
            .into_iter()
            .find(|p| p.id == pid)
            .or_else(|| extract::tiles(html).into_iter().next());

        if ld.is_none() && tile.is_none() && actions.is_empty() {
            // Nothing recognisable at all: an error page, a bot check, or a
            // redesign. Worth saying so rather than returning a hollow product.
            return Err(Error::NotInPage {
                what: format!("product {pid}"),
                detail: ", so the page was not a product page".into(),
            });
        }

        let mut product = tile.unwrap_or_default();
        if product.id.is_empty() {
            product.id = pid.to_string();
        }
        let mut detail = ProductDetail {
            product,
            description: None,
            sku: None,
            max_quantity: None,
            axes: Vec::new(),
            shipping: Vec::new(),
        };

        if let Some(ld) = ld {
            if detail.product.name.is_empty() {
                detail.product.name = ld.name.unwrap_or_default();
            }
            if detail.product.brand.is_none() {
                detail.product.brand = ld.brand.and_then(|b| b.name);
            }
            if detail.product.image.is_none() {
                detail.product.image = ld.image.into_iter().next();
            }
            // The barcode only appears here, and only on a detail page.
            if detail.product.ean.is_none() {
                detail.product.ean = ld.gtin13.into_iter().next();
            }
            if let Some(offer) = ld.offers {
                if detail.product.price.is_empty() {
                    detail.product.price = crate::domain::Price {
                        value: offer.price.as_deref().and_then(|p| p.parse().ok()),
                        formatted: offer.price.map(|p| format!("${p}")),
                        currency: offer.currency,
                    };
                }
                // schema.org states the online offer only, so it fills the
                // online axis and leaves the in-store one alone.
                if detail.product.availability.online.is_none() {
                    detail.product.availability.online =
                        offer.availability.as_deref().map(schema_in_stock);
                }
            }
            // Entities survive into the JSON-LD as written, the same as they do
            // in the variation JSON, so they are undone in both places.
            detail.description = ld.description.as_deref().map(crate::wire::unescape);
            detail.sku = ld.sku;
        }
        if detail.sku.is_none() {
            detail.sku = Some(pid.to_string());
        }

        Ok(Pdp {
            pid: pid.to_string(),
            detail,
            actions,
        })
    }

    /// When the page's tokens were minted, from the `verify` value's own
    /// timestamp half.
    pub fn minted_at(&self) -> Option<u64> {
        let url = self
            .actions
            .add_to_cart
            .as_deref()
            .or(self.actions.add_to_wishlist.as_deref())
            .or(self.actions.store_stock.as_deref())?;
        let verify = url.split("verify=").nth(1)?;
        verify.split('-').next()?.parse().ok()
    }

    /// Whether this page is old enough that its tokens are worth re-fetching
    /// before use.
    ///
    /// Advisory only -- the server decides, and the real expiry is not
    /// published. This exists so a long-running command re-reads the page
    /// rather than spending a write on a token that will be refused.
    pub fn stale(&self, max_age_secs: u64) -> bool {
        self.minted_at()
            .is_some_and(|t| net_kit::jwt::now_secs().saturating_sub(t) > max_age_secs)
    }

    /// The action URL for an operation, or the reason there is not one.
    ///
    /// A missing action is usually the site's answer rather than a parse
    /// failure: a sold-out product genuinely has no add-to-cart URL, which is
    /// worth saying plainly.
    pub fn action(&self, which: Action) -> Result<&str> {
        let slot = match which {
            Action::AddToCart => &self.actions.add_to_cart,
            Action::AddToWishlist => &self.actions.add_to_wishlist,
            Action::StoreStock => &self.actions.store_stock,
            Action::Shipping => &self.actions.shipping,
            Action::Variation => &self.actions.variation,
        };
        slot.as_deref().ok_or_else(|| Error::NotInPage {
            what: format!("{} for {}", which.describe(), self.pid),
            detail: which.why_absent(&self.detail).to_string(),
        })
    }

    /// A variation URL with one axis value chosen.
    ///
    /// The parameter name embeds the product id -- `dwvar_<pid>_<attr>` -- so
    /// it cannot be a constant. The site pre-signs one URL per value, which is
    /// preferred when present because it carries the rest of the current
    /// selection with it.
    pub fn select(&self, axis: &str, value: &str) -> Result<String> {
        if let Some(url) = self
            .detail
            .axes
            .iter()
            .find(|a| a.id == axis)
            .and_then(|a| a.values.iter().find(|v| v.id == value || v.label == value))
            .and_then(|v| v.url.clone())
        {
            return Ok(url);
        }
        let base = self.action(Action::Variation)?;
        let joiner = if base.contains('?') { '&' } else { '?' };
        Ok(format!(
            "{base}{joiner}dwvar_{}_{}={}",
            self.pid,
            axis,
            crate::endpoints::encode(value)
        ))
    }
}

/// `http://schema.org/InStock` and its neighbours.
fn schema_in_stock(availability: &str) -> bool {
    let leaf = availability.rsplit('/').next().unwrap_or(availability);
    matches!(
        leaf,
        "InStock" | "InStoreOnly" | "LimitedAvailability" | "PreOrder"
    )
}

/// The token-bearing operations a product page authorises.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Action {
    AddToCart,
    AddToWishlist,
    StoreStock,
    Shipping,
    Variation,
}

impl Action {
    pub fn describe(self) -> &'static str {
        match self {
            Action::AddToCart => "an add-to-cart action",
            Action::AddToWishlist => "an add-to-wishlist action",
            Action::StoreStock => "a store-stock action",
            Action::Shipping => "a shipping action",
            Action::Variation => "a variation action",
        }
    }

    /// The likely reason, so the message says something more useful than that a
    /// URL was missing.
    fn why_absent(self, detail: &ProductDetail) -> &'static str {
        match self {
            Action::AddToCart if detail.product.availability.online == Some(false) => {
                ". It is not orderable online, so the page offers no way to add it"
            }
            Action::Variation if detail.axes.is_empty() => {
                ". This product has no variations to choose between"
            }
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r##"
    <script type="application/ld+json">{"@context":"http://schema.org/","@type":"Product",
      "name":"Example Milk 3L","description":"Example Milk, three litres","sku":"R2000001",
      "brand":{"name":"Example Dairy"},"gtin13":["9400000000003"],
      "image":["https://example.test/milk.jpg"],
      "offers":{"@type":"Offer","price":"8.99","priceCurrency":"NZD","availability":"http://schema.org/InStock"}}</script>
    <input class="add-to-cart-url" value="/cart/add-product?pid=R2000001&amp;verify=1788496536-aaa" />
    <gep-add-to-wishlist url="/wishlist-add-product?pid=R2000001&amp;verify=1788496536-bbb"></gep-add-to-wishlist>
    <a href="/products/stores?pid=R2000001&amp;verify=1788496536-ccc">stock</a>"##;

    #[test]
    fn a_page_yields_the_product_and_the_actions_it_authorises() {
        let pdp = Pdp::parse("R2000001", PAGE).unwrap();
        assert_eq!(pdp.detail.product.name, "Example Milk 3L");
        assert_eq!(pdp.detail.product.brand.as_deref(), Some("Example Dairy"));
        // Only a detail page has the barcode.
        assert_eq!(pdp.detail.product.ean.as_deref(), Some("9400000000003"));
        assert_eq!(pdp.detail.product.price.value, Some(8.99));
        assert_eq!(pdp.detail.product.availability.online, Some(true));
        assert!(pdp.action(Action::AddToCart).is_ok());
        assert_eq!(pdp.minted_at(), Some(1_788_496_536));
    }

    #[test]
    fn a_page_that_is_not_a_product_page_is_refused_rather_than_returned_hollow() {
        let err = Pdp::parse("R1", "<html><body>Page not found</body></html>").unwrap_err();
        assert!(matches!(err, Error::NotInPage { .. }), "{err}");
    }

    #[test]
    fn a_missing_action_says_why_rather_than_naming_a_missing_url() {
        let html = r#"<script type="application/ld+json">{"@type":"Product","name":"Sold Out Thing",
            "offers":{"price":"5.00","availability":"http://schema.org/OutOfStock"}}</script>"#;
        let pdp = Pdp::parse("R3", html).unwrap();
        assert_eq!(pdp.detail.product.availability.online, Some(false));
        let err = pdp.action(Action::AddToCart).unwrap_err();
        assert!(err.to_string().contains("not orderable online"), "{err}");
    }

    #[test]
    fn a_stale_page_is_told_from_its_own_token_timestamp() {
        let mut pdp = Pdp::parse("R2000001", PAGE).unwrap();
        assert!(pdp.stale(60), "a 2026 token is long past any sane max age");
        // A page minted now is not stale.
        let fresh = format!(
            "/cart/add-product?pid=R1&verify={}-aaa",
            net_kit::jwt::now_secs()
        );
        pdp.actions.add_to_cart = Some(fresh);
        assert!(!pdp.stale(300));
    }

    #[test]
    fn selecting_a_value_prefers_the_url_the_site_already_signed() {
        // The site's own URL carries the rest of the current selection, which a
        // hand-built one would drop.
        let mut pdp = Pdp::parse("R2000001", PAGE).unwrap();
        pdp.detail.axes = vec![crate::domain::VariationAxis {
            id: "color".into(),
            name: "Color".into(),
            selected: None,
            values: vec![crate::domain::VariationValue {
                id: "RED".into(),
                label: "Red".into(),
                selected: false,
                selectable: true,
                orderable: true,
                url: Some("/products/variation?dwvar_R1_color=RED&pid=R1&verify=1-x".into()),
            }],
        }];
        assert_eq!(
            pdp.select("color", "Red").unwrap(),
            "/products/variation?dwvar_R1_color=RED&pid=R1&verify=1-x"
        );
    }

    #[test]
    fn a_hand_built_selection_encodes_the_value_and_embeds_the_pid() {
        // `dwvar_<pid>_<attr>` -- the parameter name is per product, and values
        // like `BRN L` and `CHR/MARL` need encoding.
        let mut pdp = Pdp::parse("R2000001", PAGE).unwrap();
        pdp.actions.variation = Some("/products/variation?pid=R2000001&verify=1-e".into());
        assert_eq!(
            pdp.select("color", "BRN L").unwrap(),
            "/products/variation?pid=R2000001&verify=1-e&dwvar_R2000001_color=BRN%20L"
        );
    }

    #[test]
    fn schema_org_in_store_only_still_counts_as_available() {
        assert!(schema_in_stock("http://schema.org/InStock"));
        assert!(schema_in_stock("InStoreOnly"));
        assert!(!schema_in_stock("http://schema.org/OutOfStock"));
    }
}
