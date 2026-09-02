//! The GraphQL documents.
//!
//! These are cut down from what the website sends. The site asks for
//! everything its React tree might render -- ad slots, roundel artwork, health
//! star ratings -- and its `ProductSearch` document alone is about five
//! kilobytes. Asking only for the fields this tool prints keeps the requests
//! small and, more usefully, means a field the site adds or renames somewhere
//! else cannot break a query here.
//!
//! Operation names are kept exactly as the site spells them. They travel in the
//! `wnzx-operation-name` header and the `op-name` query parameter as well as
//! the document, and there is no reason to look like a different client.

/// One product page, however it was selected.
///
/// The five ways to select products -- keyword, category, specials, buy-again
/// and product group -- are all fields of one `CompositeSearchInput`, so the
/// site uses a single document for search, browse, specials and "buy it again".
/// This does the same: [`crate::api::SearchBy`] builds the variables.
///
/// `baselineFilters` is skipped unconditionally. It exists so the website can
/// grey out filter checkboxes that would return nothing, and asking for it
/// doubles the work the server does.
pub const PRODUCT_SEARCH: &str = r#"
query ProductSearch($searchInput: CompositeSearchInput!) {
  My {
    products(searchInput: $searchInput) {
      totalCount
      totalPages
      results {
        __typename
        ... on ProductSummary { ...ProductFields }
        ... on SponsoredProduct { ...SponsoredFields }
      }
    }
  }
}

fragment ProductFields on ProductSummary {
  sku
  productName
  brand
  slug
  imageUrl
  storeKey
  isAlcohol
  isTobacco
  variants { ...VariantFields }
  categoryHierarchyNames { lvl0 lvl1 lvl2 }
}

fragment SponsoredFields on SponsoredProduct {
  sku
  productName
  brand
  slug
  imageUrl
  storeKey
  isAlcohol
  isTobacco
  variants { ...VariantFields }
  categoryHierarchyNames { lvl0 lvl1 lvl2 }
}

fragment VariantFields on VariantSummary {
  variantKey
  unitOfMeasure
  stockOnHand
  availabilityStatus
  purchaseUnit { unit minimumQty maximumQty incrementQty defaultQty }
  variantPrice {
    sellingPrice
    sellingUnit
    wasPrice
    savedAmount
    savedPercentage
    cupPrice
    cupUnit
    currency
    isSpecial
    isClubPrice
  }
}
"#;

/// The whole department tree, for turning a department name into the key
/// `byCategoryKey` wants.
pub const CATEGORIES: &str = r#"
query GetAllCategories($categoryKey: String) {
  My {
    categories(categoryKey: $categoryKey) {
      ...CategoryFields
      children {
        ...CategoryFields
        children { ...CategoryFields }
      }
    }
  }
}

fragment CategoryFields on Category {
  key
  name
  slug
  displaySlug
  level
  displayOrder
}
"#;

/// Stores, by name or near a coordinate.
pub const LOCATIONS: &str = r#"
query SearchLocations($input: LocationsInput!) {
  locations(input: $input) {
    locations {
      id
      name
      storeId
      description
      distance
      address {
        lines { line1 line2 line3 line4 line5 }
        locality { suburb city state postcode country }
      }
    }
  }
}
"#;

/// The cart as it stands.
///
/// `product` carries the name and brand; the line item itself only has SKUs and
/// money, so both are needed to render a row.
pub const CART: &str = r#"
query CustomerCart {
  customerCart { ...CartFields }
}
"#;

/// Set one or more line quantities. Zero removes a line, which is how the site
/// does it too.
pub const SET_QUANTITY: &str = r#"
mutation SetCartLineItemQuantity($input: SetCartLineItemQuantitiesInput!) {
  setCartLineItemQuantity(input: $input) { ...CartFields }
}
"#;

pub const CLEAR_CART: &str = r#"
mutation ClearCart {
  clearCart { ...CartFields }
}
"#;

/// Shared by every operation that answers with a cart, so a mutation renders
/// the same way `wwnz cart list` does.
///
/// The reads and the writes all answer with `CustomerCart`, which is what lets
/// one fragment cover the query and both mutations.
pub const CART_FIELDS: &str = r#"
fragment CartFields on CustomerCart {
  key
  totalItemQuantity
  totalUniqueProductSku
  checkout { amountToPayAsCents chargeableTotalAsCents loyaltySpendAsCents }
  pricing {
    orderSubtotal { afterDiscountAsCents discountAmountAsCents }
    productSubtotal { afterDiscountAsCents discountAmountAsCents }
  }
  validationResult {
    isValid
    failedValidations { ruleName message affectedSkus title }
  }
  lineItems {
    sku
    productVariantSku
    quantity
    canSubstitute
    lineTotal { afterDiscountAsCents discountAmountAsCents }
    unitPrice { afterDiscountAsCents }
    product {
      brand
      slug
      isLiquor
      isTobacco
      variants { ... on GroceryVariant { name key } ... on RegulatedVariant { name key } }
    }
  }
  shoppingMode {
    pickupLocation { id name }
  }
  fulfilment {
    propositionId
    fulfilmentProposition { id storeId method name store { storeId name } }
  }
}
"#;

/// Choose the store to shop against.
///
/// This is a cart mutation rather than a preference: on this site the store is
/// a property of the cart's fulfilment, and it is what the pricing in a search
/// response is keyed to. Setting it works for a guest as well as an account.
pub const SET_SHOPPING_MODE: &str = r#"
mutation SetCartShoppingMode($setCartShoppingModeInput: SetCartShoppingModeInput!) {
  setCartShoppingMode(input: $setCartShoppingModeInput) {
    fulfilment {
      propositionId
      fulfilmentProposition { id storeId method store { storeId name } }
    }
    shoppingMode { pickupLocation { id name } }
    validationResult { isValid failedValidations { message title } }
  }
}
"#;

/// Past orders, newest first.
pub const ORDERS: &str = r#"
query Orders($input: OrdersInput!) {
  orders(input: $input) {
    totalCount
    totalPages
    pageSize
    currentPage
    results {
      orderNumber
      createdDateTime
      orderStatus
      fulfilmentStatus
      hasInvoice
      isAmendable
      total { afterDiscountInCents }
      fulfilments {
        method
        startTime
        endTime
        fulfilmentLocation { name }
        address { lines { line1 line2 line3 } }
      }
    }
  }
}
"#;

/// A document plus every fragment it refers to.
///
/// GraphQL has no include directive, so a fragment shared between operations
/// has to be concatenated onto each one that uses it.
pub fn document(operation: &str) -> String {
    match operation {
        "CustomerCart" => format!("{CART}{CART_FIELDS}"),
        "SetCartLineItemQuantity" => format!("{SET_QUANTITY}{CART_FIELDS}"),
        "ClearCart" => format!("{CLEAR_CART}{CART_FIELDS}"),
        "ProductSearch" => PRODUCT_SEARCH.to_string(),
        "GetAllCategories" => CATEGORIES.to_string(),
        "SearchLocations" => LOCATIONS.to_string(),
        "SetCartShoppingMode" => SET_SHOPPING_MODE.to_string(),
        "Orders" => ORDERS.to_string(),
        other => panic!("no GraphQL document for operation {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every operation the client asks for has to assemble, and the ones using
    /// the cart fragment have to actually carry it -- a missing fragment is a
    /// server-side error, not a compile-time one.
    #[test]
    fn every_operation_assembles_with_its_fragments() {
        for op in [
            "CustomerCart",
            "SetCartLineItemQuantity",
            "ClearCart",
            "ProductSearch",
            "GetAllCategories",
            "SearchLocations",
            "SetCartShoppingMode",
            "Orders",
        ] {
            let doc = document(op);
            assert!(doc.contains(op), "{op} document does not define {op}");
            if doc.contains("...CartFields") {
                assert!(
                    doc.contains("fragment CartFields"),
                    "{op} uses CartFields without defining it"
                );
            }
        }
    }

    #[test]
    fn the_search_document_defines_the_fragments_it_uses() {
        for fragment in ["ProductFields", "SponsoredFields", "VariantFields"] {
            assert!(
                PRODUCT_SEARCH.contains(&format!("fragment {fragment}")),
                "{fragment} is used but not defined"
            );
        }
    }
}
