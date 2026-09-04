//! The basket and the wishlist.
//!
//! The one half of this storefront that was already an API: these controllers
//! answer typed JSON with line items, prices and totals. What they still need
//! is a `verify` token from a product page, so every write here starts from a
//! [`crate::Pdp`].

use crate::error::{Error, Result};
use crate::wire;

/// Read an action's answer, turning the site's own refusal into an error.
///
/// SFCC answers a refused action with HTTP 200 and `error: true`, so the status
/// code says nothing and the body has to be inspected either way.
pub fn checked(action: &'static str, body: &str) -> Result<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| Error::decode(format!("parsing {action}"), e))?;
    let envelope: wire::Envelope = serde_json::from_value(value.clone()).unwrap_or_default();
    if envelope.error {
        let message = envelope
            .msg
            .unwrap_or_else(|| "the site gave no reason".into());
        // An expired page token comes back as an ordinary refusal, but its fix
        // is to re-read the page rather than to change anything about the
        // request -- so it is worth telling apart here.
        if is_stale_token(&message) {
            return Err(Error::TokenExpired { action });
        }
        return Err(Error::Refused { action, message });
    }
    Ok(value)
}

/// Whether a refusal is about the page token rather than the request.
///
/// Deliberately does *not* include `Cross-Origin Request Blocked`. That message
/// is literal -- the site is objecting to the request's `Sec-Fetch-*` headers,
/// not to its token -- and treating it as staleness would spend a wasted page
/// fetch on every occurrence and then fail the same way. See
/// [`crate::Client`]'s action headers.
fn is_stale_token(message: &str) -> bool {
    let m = message.to_lowercase();
    (m.contains("verify") || m.contains("token") || m.contains("expired"))
        && !m.contains("out of stock")
}

pub fn cart_from(value: serde_json::Value) -> Result<crate::Cart> {
    let parsed: wire::CartResponse =
        serde_json::from_value(value).map_err(|e| Error::decode("parsing the cart", e))?;
    Ok(parsed.into_cart())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_refused_with_a_200_is_still_an_error() {
        // The whole reason the body is read: SFCC does not use the status code
        // for this, so a naive client would report success.
        let err = checked(
            "wishlist add",
            r#"{"action":"Wishlist-AddProduct","error":true,
                "msg":"The product is already in the wishlist and will not be added again."}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("already in the wishlist"), "{err}");
        assert!(!err.is_stale_token());
    }

    #[test]
    fn an_expired_page_token_is_told_apart_from_an_ordinary_refusal() {
        let err = checked(
            "cart add",
            r#"{"error":true,"msg":"The verify token has expired."}"#,
        )
        .unwrap_err();
        assert!(err.is_stale_token(), "{err}");
    }

    #[test]
    fn a_blocked_request_is_not_mistaken_for_a_stale_token() {
        // The site means this one literally: it is the `Sec-Fetch-*` headers,
        // not the token. Retrying with a fresh page would fail identically.
        let err = checked(
            "store stock",
            r#"{"error":true,"errorMessage":"Cross-Origin Request Blocked"}"#,
        )
        .unwrap_err();
        assert!(!err.is_stale_token(), "{err}");
        assert!(err.to_string().contains("Cross-Origin"), "{err}");
    }

    #[test]
    fn a_stock_refusal_is_not_mistaken_for_a_stale_token() {
        // "expired" appears in both kinds of message; retrying a sold-out add
        // with a fresh page would just fail again.
        let err = checked(
            "cart add",
            r#"{"error":true,"msg":"This offer has expired -- the product is out of stock."}"#,
        )
        .unwrap_err();
        assert!(!err.is_stale_token(), "{err}");
    }

    #[test]
    fn a_successful_action_yields_its_cart() {
        let value = checked(
            "cart add",
            r#"{"error":false,"cartModel":{"cartId":"c1","subTotal":"$10.48",
                "items":[{"uuid":"u1","id":"R1","productName":"Thing","quantity":2,
                "price":{"sales":{"value":7.49,"formatted":"$7.49"}}}]}}"#,
        )
        .unwrap();
        let cart = cart_from(value).unwrap();
        assert_eq!(cart.lines.len(), 1);
        assert_eq!(cart.quantity, 2);
    }
}
