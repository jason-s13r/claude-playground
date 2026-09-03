//! The GraphQL documents.
//!
//! These are cut down from what the website sends. The site asks for everything
//! its React tree might render -- ad slots, roundel artwork, health star
//! ratings -- and its `ProductSearch` document alone is about five kilobytes.
//! Asking only for the fields this crate returns keeps the requests small and,
//! more usefully, means a field the site adds or renames somewhere else cannot
//! break a query here.
//!
//! Operation names are kept exactly as the site spells them. They travel in the
//! `wnzx-operation-name` header and the `op-name` query parameter as well as
//! the document, and there is no reason to look like a different client.

/// One product page, however it was selected.
///
/// The ways to select products -- keyword, category, specials and buy-again --
/// are all fields of one `CompositeSearchInput`, so the site uses a single
/// document for search, browse, specials and "buy it again". This does the
/// same; [`crate::SearchBy`] builds the variables.
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
  variants { ...VariantFields }
  categoryHierarchyNames { lvl0 lvl1 lvl2 }
}

fragment VariantFields on VariantSummary {
  variantKey
  unitOfMeasure
  availabilityStatus
  variantPrice {
    sellingPrice
    wasPrice
    cupPrice
    cupUnit
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
      distance
      address {
        lines { line1 line2 line3 line4 line5 }
        locality { suburb city state postcode country }
      }
    }
  }
}
"#;

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
/// the same way a read does. The reads and the writes all answer with
/// `CustomerCart`, which is what lets one fragment cover all three.
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
/// A cart mutation rather than a preference: on this site the store is a
/// property of the cart's fulfilment, and it is what the pricing in a search
/// response is keyed to. It works for a guest as well as an account.
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

/// One order and what was in it.
///
/// Cut down hard from the site's own document, which also asks for the
/// customer's name, email and phone, and for each payment's card suffix. None
/// of that is needed to show an order, and asking for it would mean holding it.
pub const ORDER_DETAILS: &str = r#"
query OrderDetails($orderNumber: ID!) {
  order(orderNumber: $orderNumber) {
    orderNumber
    orderStatus
    createdDateTime
    isAmendable
    orderTotalInCents
    estimatedTotalInCents
    orderDiscountInCents
    orderSavingsInCents
    productSubtotal { beforeDiscountInCents afterDiscountInCents discountAmountInCents }
    fees { type tag amountInCents }
    fulfilments {
      method
      type
      kind
      startTime
      endTime
      fulfilmentLocation { name storeId }
      address { lines { line1 line2 line3 } }
    }
    lineItems {
      productId
      productKey
      skuId
      quantity
      allowSubstitutions
      totalPriceAsCents
      totalSavingAsCents
      unitPriceAfterDiscountAsCents
      product { name }
    }
  }
}
"#;

/// Every operation this crate can send.
pub const OPERATIONS: [&str; 9] = [
    "ProductSearch",
    "GetAllCategories",
    "SearchLocations",
    "CustomerCart",
    "SetCartLineItemQuantity",
    "ClearCart",
    "SetCartShoppingMode",
    "Orders",
    "OrderDetails",
];

/// A document plus every fragment it refers to.
///
/// GraphQL has no include directive, so a fragment shared between operations
/// has to be concatenated onto each one that uses it.
pub fn document(operation: &str) -> Option<String> {
    Some(match operation {
        "CustomerCart" => format!("{CART}{CART_FIELDS}"),
        "SetCartLineItemQuantity" => format!("{SET_QUANTITY}{CART_FIELDS}"),
        "ClearCart" => format!("{CLEAR_CART}{CART_FIELDS}"),
        "ProductSearch" => PRODUCT_SEARCH.to_string(),
        "GetAllCategories" => CATEGORIES.to_string(),
        "SearchLocations" => LOCATIONS.to_string(),
        "SetCartShoppingMode" => SET_SHOPPING_MODE.to_string(),
        "Orders" => ORDERS.to_string(),
        "OrderDetails" => ORDER_DETAILS.to_string(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every operation the client can ask for has to assemble. A missing
    /// fragment is a server-side error, not a compile-time one, so it is worth
    /// a test.
    #[test]
    fn every_operation_has_a_document_that_defines_itself() {
        for op in OPERATIONS {
            let doc = document(op).unwrap_or_else(|| panic!("no document for {op}"));
            assert!(doc.contains(op), "{op} document does not define {op}");
        }
    }

    #[test]
    fn every_fragment_used_is_also_defined() {
        for op in OPERATIONS {
            let doc = document(op).expect("checked above");
            for spread in doc.match_indices("...").map(|(i, _)| &doc[i + 3..]) {
                let name: String = spread
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if name.is_empty() {
                    continue; // `... on Type`, an inline fragment
                }
                assert!(
                    doc.contains(&format!("fragment {name}")),
                    "{op} spreads {name} without defining it"
                );
            }
        }
    }

    #[test]
    fn an_unknown_operation_is_none_rather_than_a_panic() {
        assert!(document("NoSuchOperation").is_none());
    }

    #[test]
    fn the_order_detail_document_does_not_ask_for_personal_details() {
        // The site's own document asks for contact name, email, phone and each
        // payment's card suffix. None of it is needed to show an order, and
        // asking would mean holding it.
        for field in ["contact", "payments", "cardSuffix", "firstName", "email"] {
            assert!(!ORDER_DETAILS.contains(field), "asks for {field}");
        }
    }
}
