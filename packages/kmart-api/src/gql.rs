//! The GraphQL documents.
//!
//! Cut down from what the website sends. Its `getMyActiveCart` alone asks for
//! ad slots, OnePass eligibility and logistics ids across two and a half
//! kilobytes; asking only for the fields this crate returns keeps the requests
//! small and, more usefully, means a field Kmart adds or renames somewhere
//! else cannot break a query here.
//!
//! Operation names are kept exactly as the site spells them, including where
//! the spelling is wrong -- `createMyShoppingList` *adds an item to* a list
//! and creates nothing. They are what the gateway logs and there is no reason
//! to look like a different client.
//!
//! `__typename` is dropped throughout. The site's Apollo cache needs it; a
//! client that reads the fields it asked for does not.

/// Stock, everywhere, for one or more keycodes near a postcode.
///
/// The whole stock story in one call. `CLICK_AND_COLLECT` answers with a
/// location id per store and no name, which is why [`LOCATION_DETAIL`] exists
/// and why anything showing stores makes N+1 requests by design.
///
/// The four channels are shaped differently and not by accident: the two that
/// ship have a single pooled `stock`, and the two that involve walking into a
/// building have `locations`. Asking for `stock` on `IN_STORE` is a schema
/// error, not an empty answer. The two `locations` shapes differ again in the
/// small: only the collect one names a location on its `fulfilment`, so the
/// id has to be read from `location` as well.
pub const PRODUCT_AVAILABILITY: &str = r#"
query getProductAvailability($input: ProductAvailabilityQueryInput!) {
  getProductAvailability(input: $input) {
    postcode
    country
    region
    availability {
      HOME_DELIVERY { poolName stock { available } }
      CLICK_AND_COLLECT {
        stock { totalAvailable }
        locations {
          fulfilment { isBuddyLocation locationId stock { available } }
          location { locationId }
          distanceInKm
        }
      }
      IN_STORE {
        locations {
          fulfilment { stock { available } }
          location { locationId }
        }
      }
      EXPRESS_DELIVERY { stock { available } }
    }
  }
}
"#;

/// One store, by the id an availability answer named.
pub const LOCATION_DETAIL: &str = r#"
query getLocationDetail($input: LocationQueryInput!) {
  locationQuery(input: $input) {
    publicName
    phoneNumber
    address1
    address2
    address3
    city
    state
    postcode
    latitude
    longitude
    tradingHours { weekDay hours }
  }
}
"#;

/// Postcode to suburb and subdivision.
///
/// Also the cheapest possible check that the gateway is reachable and
/// unchallenged, which is what `doctor` uses it for: it needs no account and
/// takes one argument.
pub const POSTCODE_SUGGESTIONS: &str = r#"
query getPostcodeSuggestions($input: PostcodeQueryInput!) {
  postcodeQuery(input: $input) { postcode state suburb isMetroRegionForFreeShipping }
}
"#;

/// The signed-in cart.
///
/// Answers `null` for a visitor rather than failing, which is the tell that
/// there is no session: an anonymous cart exists only as an id the browser
/// kept, and `me` cannot find it.
pub const ACTIVE_CART: &str = r#"
query getMyActiveCart {
  me {
    activeCart {
      id
      version
      selectedCncStoreId
      totalPrice { centAmount }
      shippingInfo { shippingMethodName shippingRate { price { centAmount } } }
      postcodeSelector { postalCode state city country }
      lineItems {
        id
        name(locale: "en")
        quantity
        price { value { centAmount } }
        totalPrice { centAmount }
        variant { sku imageKey attributes { name value } }
        custom { fields { sellerName } }
      }
    }
  }
}
"#;

/// Start a cart. Needed because a signed-in shopper with no cart has none to
/// update, and `updateMyCart` cannot make one.
pub const CREATE_CART: &str = r#"
mutation createMyBag($draft: MyCartDraft!) {
  createMyCart(draft: $draft) {
    id
    version
    postcodeSelector { postalCode }
  }
}
"#;

/// Change a cart.
///
/// Optimistic concurrency: `version` is the version last read, and the gateway
/// refuses the write if anything else moved it on. That is why every mutation
/// here is a read-then-write and why [`crate::Error::CartConflict`] exists.
pub const UPDATE_CART: &str = r#"
mutation updateMyBag($id: String!, $version: Long!, $actions: [MyCartUpdateAction!]!) {
  updateMyCart(id: $id, version: $version, actions: $actions) {
    id
    version
    totalPrice { centAmount }
    shippingInfo { shippingMethodName shippingRate { price { centAmount } } }
    lineItems {
      id
      name(locale: "en")
      quantity
      price { value { centAmount } }
      totalPrice { centAmount }
      variant { sku imageKey attributes { name value } }
      custom { fields { sellerName } }
    }
  }
}
"#;

/// What is saved for later.
///
/// Note `name(locale: "en-AU")` -- the site sends the Australian locale for
/// the New Zealand storefront too, and a locale the gateway does not know
/// answers with an empty name rather than an error, so this is copied rather
/// than corrected.
pub const WISHLIST: &str = r#"
query GetWishListItems($countryCode: Country!) {
  me {
    defaultShoppingList {
      id
      version
      key
      lineItems {
        id
        name(locale: "en-AU")
        quantity
        variant {
          id
          sku
          imageKey
          shoppingListPrice(input: {country: $countryCode}) { centAmount }
        }
        custom { fields { offerId } }
      }
    }
  }
}
"#;

/// The same, plus whether each item can actually be had.
///
/// A different field on `me` -- `defaultWishlist`, not `defaultShoppingList`
/// -- returning the same list through a resolver that also prices
/// availability. Costs a postcode and more time, so it is a separate document
/// rather than the default.
pub const WISHLIST_WITH_STOCK: &str = r#"
query GetWishListItemsWithStockAvailability($postcode: String!, $countryCode: Country!) {
  me {
    defaultWishlist {
      id
      version
      key
      lineItemsWithStockAvailabilityStatus(
        input: {country: $countryCode, postcode: $postcode}
      ) {
        id
        name
        quantity
        stockAvailability {
          isStockAvailable
          stockLevel
          isCncOnly
          isDeliveryOnly
          isInStoreOnly
          isComingSoon
          isPreOrderActive
          preOrderReleaseDate
        }
        variant {
          id
          sku
          imageKey
          shoppingListPrice { centAmount }
        }
      }
    }
  }
}
"#;

/// Save a product.
///
/// The operation name is the site's and it is a misnomer: this adds an item to
/// the existing list, and `version` is optional precisely because the list is
/// created on first use.
pub const WISHLIST_ADD: &str = r#"
mutation createMyShoppingList($quantity: Int!, $sku: String!, $version: Long, $offerId: String) {
  addItemToShoppingList(version: $version, sku: $sku, quantity: $quantity, offerId: $offerId) {
    id
    version
    lineItems { id quantity variant { id sku } }
  }
}
"#;

/// Past orders, newest first. `startsAfter` pages it.
pub const ORDERS: &str = r#"
query getOrdersForCustomer($startsAfter: String, $limit: Float) {
  getOrdersForCustomer(startsAfter: $startsAfter, limit: $limit) {
    count
    startsAfter
    orders {
      displayOrderId
      orderTotal
      orderStatus
      orderDate
      shippedItems { trackingNumber carrier trackingLink }
    }
  }
}
"#;

/// Who is signed in. The cheapest account-scoped call there is, which is what
/// makes it the one `auth status` uses to prove a token really works.
pub const CUSTOMER: &str = r#"
query getDetails {
  me { customer { id email firstName lastName } }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [(&str, &str); 11] = [
        ("PRODUCT_AVAILABILITY", PRODUCT_AVAILABILITY),
        ("LOCATION_DETAIL", LOCATION_DETAIL),
        ("POSTCODE_SUGGESTIONS", POSTCODE_SUGGESTIONS),
        ("ACTIVE_CART", ACTIVE_CART),
        ("CREATE_CART", CREATE_CART),
        ("UPDATE_CART", UPDATE_CART),
        ("WISHLIST", WISHLIST),
        ("WISHLIST_WITH_STOCK", WISHLIST_WITH_STOCK),
        ("WISHLIST_ADD", WISHLIST_ADD),
        ("ORDERS", ORDERS),
        ("CUSTOMER", CUSTOMER),
    ];

    #[test]
    fn every_document_has_balanced_braces() {
        // A trimmed document is trimmed by hand, and an unbalanced one fails
        // as a parse error from the gateway rather than as anything readable.
        for (name, doc) in ALL {
            let opens = doc.matches('{').count();
            let closes = doc.matches('}').count();
            assert_eq!(opens, closes, "{name} has {opens} open and {closes} close");
        }
    }

    #[test]
    fn every_document_names_its_operation() {
        for (name, doc) in ALL {
            let first = doc.trim_start().lines().next().unwrap();
            assert!(
                first.starts_with("query ") || first.starts_with("mutation "),
                "{name} starts with {first:?}"
            );
            // Two of these take no arguments at all -- the cart and the
            // customer are both scoped by the bearer token alone.
            let named = first
                .split_once(' ')
                .map(|(_, rest)| rest.trim_start())
                .unwrap_or("");
            assert!(
                named.starts_with(|c: char| c.is_ascii_alphabetic()),
                "{name} is anonymous: {first}"
            );
        }
    }

    #[test]
    fn no_document_asks_for_typename() {
        // The site sends it for Apollo's cache. Nothing here reads it, and it
        // is pure payload.
        for (name, doc) in ALL {
            assert!(!doc.contains("__typename"), "{name}");
        }
    }
}
