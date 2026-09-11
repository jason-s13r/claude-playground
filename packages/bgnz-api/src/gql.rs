//! The GraphQL documents.
//!
//! Cut down from what the storefront sends. Its `getProductListing` alone asks
//! for free-gift milestones, badge colours and configurable variant matrices
//! across three kilobytes; asking only for the fields this crate returns keeps
//! the requests small and, more usefully, means a field Briscoes adds or
//! renames somewhere else cannot break a query here.
//!
//! Operation names are kept exactly as the site spells them, inconsistent
//! casing included -- `GetCart` beside `getPriceSummary`, `GET_STORES` beside
//! `getHistoryOrderList`. They are what the storefront logs, and there is no
//! reason to look like a different client.
//!
//! `__typename` is dropped throughout. The site's Apollo cache needs it; a
//! client that reads the fields it asked for does not.
//!
//! Two documents deserve their own note, and have one below: `LoginGigya`,
//! which is the only way an account reaches Magento, and `CustomerOrdersHistory`,
//! which is a different history from `getHistoryOrderList` rather than another
//! page of it.

// ------------------------------------------------------------------ storefront

/// Where Klevu answers for this fascia, and under which key.
///
/// Fetched rather than hardcoded: both values are per-fascia, both are public,
/// and the shard in `aucs34.ksearchnet.com` is an assignment that can move. A
/// copy in this crate would go stale silently -- Klevu answers an unknown key
/// with an empty result set, not an error.
pub const KLEVU_DATA: &str = r#"
query KlevuData {
  storeConfig { store_code klevu_search_url klevu_search_js_api_key }
}
"#;

/// Which SAP Customer Data Cloud site this fascia authenticates against.
///
/// The two fascias have different API keys against the same datacenter, which
/// is the whole reason a Briscoes sign-in is not a Rebel Sport one.
pub const STORE_CONFIG_FOR_GIGYA: &str = r#"
query GetStoreConfigForGigya {
  storeConfig {
    store_code
    gigya_enable
    gigya_api_key
    gigya_data_center
    gigya_login_screen_set
    gigya_login_start_screen
  }
}
"#;

/// How long a minted customer token is good for, in minutes.
pub const TIME_LIFE_TOKEN: &str = r#"
query GetTimeLifeToken {
  storeConfig { store_code customer_token_timeout }
}
"#;

// -------------------------------------------------------------------- catalogue

/// One product, by SKU.
///
/// The fields after `url_key` are Briscoes' own additions, and four of them
/// exist only to be handed straight back to the click-and-collect API --
/// `barcode`, `sapcategory`, `subcategory` and `shipping_band`. That is why
/// [`crate::Client::stock`] takes a product rather than a SKU: the availability
/// service will not answer without them.
///
/// The variants block is there for the same reason and not for display. A
/// configurable product carries `barcode: null` and hangs the real barcodes off
/// its variants, so without this block every Rebel Sport shoe and shirt would
/// be unanswerable at a store.
pub const PRODUCT_DETAIL: &str = r#"
query getProductDetailForProductPageBySku($sku: String) {
  products(filter: { sku: { eq: $sku } }) {
    total_count
    items {
      uid
      sku
      name
      url_key
      url_suffix
      stock_status
      brand
      brand_url
      product_description
      image { url }
      media_gallery { url }
      barcode
      sapcategory
      subcategory
      shipping_band
      isdropship
      isclickcollectnotavailable
      productisclickcollectnotavailable
      saleavailability_label
      categories { uid name level }
      ... on ConfigurableProduct {
        variants {
          attributes { code label }
          product {
            sku
            stock_status
            barcode
            sapcategory
            subcategory
            shipping_band
            isdropship
            isclickcollectnotavailable
          }
        }
      }
    }
  }
}
"#;

/// The specifications a product page prints.
///
/// A separate query because the storefront keeps it separate, and because the
/// obvious alternative is worse: `custom_attributesV2` answers with the
/// merchandising bag -- image paths, `assortedproduct: 0`, internal category
/// codes -- even filtered to `is_visible_on_front`. This is the list a person
/// would recognise, already split into the handful worth leading with and the
/// rest.
pub const PRODUCT_SPECS: &str = r#"
query GetProductSpecs($sku: String!) {
  product_specs(input: { sku: $sku }) {
    key_specs { spec_text spec_value }
    general_specs { spec_text spec_value }
  }
}
"#;

/// Prices for one or more SKUs.
///
/// Separate from the detail query because the site keeps it separate: price is
/// customer-group dependent -- a loyalty member sees `member_price_display`
/// where a guest sees `price_display` -- so it is the one part of a product
/// that changes when a token is presented. Taking a list rather than the site's
/// single SKU because hydrating a page of Klevu results is the common case and
/// thirty-five round trips for it would be absurd.
pub const PRODUCT_PRICING: &str = r#"
query getProductPricingBySku($skus: [String]) {
  products(filter: { sku: { in: $skus } }) {
    items {
      sku
      stock_status
      price_range {
        minimum_price { final_price { currency value } regular_price { currency value } }
        maximum_price { final_price { currency value } discount { amount_off percent_off } }
      }
      price_display { disclaimer_text rule_percent badge_types { type text } }
      member_price_display {
        member_price { disclaimer_text rule_percent badge_types { type text } }
      }
      product_salesrule_badges { badge_name badge_description }
    }
  }
}
"#;

/// A pasted URL, resolved to what it points at.
///
/// The only way in from a link. Answers a `type` of `PRODUCT`, `CATEGORY` or
/// `CMS_PAGE` plus the uid, which is what every other query here needs.
pub const RESOLVE_URL: &str = r#"
query ResolveURL($url: String!) {
  route(url: $url) {
    relative_url
    type
    ... on ProductInterface { uid sku name }
    ... on CategoryInterface { uid name }
  }
}
"#;

/// The category tree, flat.
///
/// `categories(filters: {})` answers the lot -- 961 for Briscoes, 1004 for
/// Rebel Sport -- which is small enough to fetch whole and shape here rather
/// than walking it a level at a time. The uid is base64 of the numeric id, and
/// that number is what Klevu knows a category by; see
/// [`crate::search::Query::category`].
pub const CATEGORIES: &str = r#"
query GetCategories {
  categories(filters: {}, pageSize: 2000) {
    total_count
    items { uid name level url_path children_count }
  }
}
"#;

// ----------------------------------------------------------------------- stores

/// Every store, grouped by region, with addresses and hours.
pub const STORES: &str = r#"
query GET_STORES {
  getRegion(includeStore: true) {
    region_name
    region_id
    store_items {
      store_id
      store_locator_name
      display_name
      fulfilment_number
      line1
      line2
      city
      postcode
      phone
      working_time
    }
  }
}
"#;

/// The click-and-collect facts a store list does not carry: whether a shop
/// collects at all, and its same-day cutoff.
pub const STORES_CLICK_AND_COLLECT: &str = r#"
query GetStoreClickAndCollect {
  getStoreLocator {
    store_id
    store_locator_name
    store_number
    fulfilment_number
    same_day_delivery
    is_click_and_collect
  }
}
"#;

/// Remember a store against the account.
///
/// Server-side, not a local preference: the storefront files it on the customer
/// record, so a store chosen here is the store the website shows too.
pub const SET_STORE_LOCATOR: &str = r#"
mutation SetCustomerStoreLocator($fulfilment_number: String!) {
  setStoreLocator(fulfilment_number: $fulfilment_number) { message results }
}
"#;

/// Which store the account has chosen.
pub const CUSTOMER_STORE: &str = r#"
query GetCustomerSelectedFulfilmentNumber {
  customer { custom_attributes { code ... on AttributeValue { value } } }
}
"#;

// ------------------------------------------------------------------------- auth

/// Exchange a Gigya identity assertion for a Magento customer token.
///
/// The only door in. Magento does not verify a password here -- it verifies
/// `UIDSignature`, an HMAC that Gigya computed over the UID and a timestamp --
/// so what this needs is a *fresh* signature, and freshness is why
/// [`crate::auth`] calls `accounts.getAccountInfo` before every exchange rather
/// than storing the signature it got at login.
///
/// `UID` here is the numeric SAP customer id, not a Gigya GUID: these sites use
/// custom UIDs, so the value to send is Gigya's `loginProviderUID`.
pub const LOGIN_GIGYA: &str = r#"
mutation LoginGigya($email: String!, $gigyaUid: String!, $gigyaUidSignature: String!, $signatureTimestamp: String!) {
  loginGigya(
    input: {
      UID: $gigyaUid
      email: $email
      UIDSignature: $gigyaUidSignature
      signatureTimestamp: $signatureTimestamp
    }
  ) { token }
}
"#;

/// Extend the customer token without touching Gigya.
///
/// Cheaper than a re-exchange and needs no captcha, but it only works while the
/// current token is still valid -- it renews, it does not resurrect. Once the
/// token is past `customer_token_timeout` the way back is `LoginGigya`.
pub const REFRESH_CUSTOMER_TOKEN: &str = r#"
mutation refreshCustomerToken { refreshCustomerToken { token } }
"#;

/// Who the held token belongs to.
///
/// Also the cheapest possible check that a token is live, which is what
/// `auth status` and `doctor` use it for.
pub const CUSTOMER: &str = r#"
query GetCustomerAfterSignIn {
  customer {
    email
    firstname
    lastname
    is_subscribed
    customer_group { group_code group_id }
    custom_attributes { code ... on AttributeValue { value } }
  }
}
"#;

pub const SIGN_OUT: &str = r#"
mutation SignOutFromMenu { revokeCustomerToken { result } }
"#;

// ------------------------------------------------------------------------- cart

/// The cart, with enough of each line to price it and to ask about stock.
///
/// The click-and-collect fields ride along for the same reason they do on a
/// product: `bgnz stock` on a whole cart is one availability call, and it needs
/// them per line.
pub const CART_FRAGMENT: &str = r#"
fragment CartDetail on Cart {
  id
  total_quantity
  items {
    uid
    quantity
    product {
      uid sku name url_key stock_status brand
      image { url }
      barcode sapcategory subcategory shipping_band
      isdropship isclickcollectnotavailable saleavailability_label
    }
    prices {
      price { value currency }
      price_including_tax { value currency }
      row_total { value currency }
      row_total_including_tax { value currency }
      total_item_discount { value currency }
    }
    errors { code message }
  }
  prices {
    grand_total_exclude_gift_card { value currency }
    subtotal_including_tax { value currency }
    discounts { label amount { value currency } }
  }
  applied_coupons { code }
}
"#;

pub const GET_CART: &str = r#"
query GetCartDetails($cartId: String!) {
  cart(cart_id: $cartId) { ...CartDetail }
}
"#;

pub const CREATE_CART: &str = r#"
mutation CreateCartAfterSignIn { cartId: createEmptyCart }
"#;

/// The signed-in account's own cart, which is the one the website shows.
pub const CUSTOMER_CART: &str = r#"
query createCustomerCartFromCart { customerCart { id } }
"#;

pub const ADD_TO_CART: &str = r#"
mutation AddProductToCart($cartId: String!, $product: CartItemInput!) {
  addProductsToCart(cartId: $cartId, cartItems: [$product]) {
    cart { ...CartDetail }
    user_errors { code message }
  }
}
"#;

pub const UPDATE_CART_ITEM: &str = r#"
mutation updateItemQuantity($cartId: String!, $itemId: ID!, $quantity: Float!) {
  updateCartItems(input: { cart_id: $cartId, cart_items: [{ cart_item_uid: $itemId, quantity: $quantity }] }) {
    cart { ...CartDetail }
  }
}
"#;

pub const REMOVE_CART_ITEM: &str = r#"
mutation removeItem($cartId: String!, $itemId: ID!) {
  removeItemFromCart(input: { cart_id: $cartId, cart_item_uid: $itemId }) {
    cart { ...CartDetail }
  }
}
"#;

pub const APPLY_COUPON: &str = r#"
mutation applyCouponsToCart($cartId: String!, $codes: [String]!) {
  applyCouponsToCart(input: { cart_id: $cartId, coupon_codes: $codes, type: APPEND }) {
    cart { ...CartDetail }
  }
}
"#;

// --------------------------------------------------------------------- wishlist

pub const WISHLIST: &str = r#"
query GetCustomerWishlist($currentPage: Int = 1) {
  customer {
    wishlists {
      id
      items_count
      items_v2(currentPage: $currentPage, pageSize: 200) {
        items {
          id
          quantity
          product { uid sku name url_key stock_status brand image { url } }
        }
        page_info { current_page total_pages }
      }
    }
  }
}
"#;

/// `wishlistId: "0"` means the default list, which is the only list either site
/// makes in practice -- `getMultipleWishlistsEnabled` answers false for both.
pub const ADD_TO_WISHLIST: &str = r#"
mutation AddProductToWishlistFromGallery($wishlistId: ID!, $itemOptions: WishlistItemInput!) {
  addProductsToWishlist(wishlistId: $wishlistId, wishlistItems: [$itemOptions]) {
    wishlist { id items_count }
    user_errors { code message }
  }
}
"#;

pub const REMOVE_FROM_WISHLIST: &str = r#"
mutation RemoveProductsFromWishlist($wishlistId: ID!, $wishlistItemsId: [ID!]!) {
  removeProductsFromWishlist(wishlistId: $wishlistId, wishlistItemsIds: $wishlistItemsId) {
    wishlist { id items_count }
    user_errors { message }
  }
}
"#;

// ----------------------------------------------------------------------- orders

/// Orders placed **online**, from Magento's own sales records.
pub const ORDER_LIST: &str = r#"
query getHistoryOrderList($currentPage: Int = 1, $pageSize: Int = 20) {
  getHistoryOrderList(currentPage: $currentPage, pageSize: $pageSize) {
    items {
      id
      number
      order_date
      shipping_code
      shipping_address { store_locator_name }
      custom_attribute_view { custom_status }
      total { base_grand_total_exclude_gift_card { value currency } }
    }
    page_info { total_pages current_page page_size }
  }
}
"#;

/// One online order, with its shipments and their carrier tracking.
pub const ORDER_DETAIL: &str = r#"
query getOrderDetailsFromOrderHistory($number: String!) {
  customer {
    orders(filter: { number: { eq: $number } }) {
      items {
        number
        order_date
        shipping_code
        custom_attribute_view { custom_status }
        shipping_address { firstname lastname street city postcode store_locator_name }
        custom_shipments {
          shipment_group_name
          status
          tracking_link
          items {
            product_name
            qty
            product_sale_price { value currency }
            total { value currency }
          }
        }
        payment_methods { name type }
        invoices { id number }
        total {
          grand_total { value currency }
          subtotal_incl_tax { value currency }
          shipping_handling { amount_including_tax { value currency } }
          discounts { label amount { value currency } }
          loyalty_reward { amount { value currency } }
        }
      }
    }
  }
}
"#;

/// Receipts from **walking into a shop**, out of SAP rather than Magento.
///
/// A different history from [`ORDER_LIST`], not another page of it: these are
/// till receipts matched to the loyalty membership, they carry a store name and
/// a receipt reference where an online order carries a tracking link, and the
/// two never overlap. The default window is a rolling year -- the answer's own
/// `start_date`/`end_date` say which -- and going further back means asking for
/// it explicitly, which is what `message_type: LOAD_MORE_PAST_DATA_MESSAGE_TYPE`
/// is telling the page.
pub const IN_STORE_ORDERS: &str = r#"
query CustomerOrdersHistory($page: Int, $startDate: String, $endDate: String, $reference: String) {
  customer {
    customerEEEOrdersHistory(
      page_number: $page
      filter: { reference: $reference, start_date: $startDate, end_date: $endDate }
    ) {
      success
      message
      message_type
      items {
        order_number
        order_location
        receipt_reference
        order_date
        total_line_items
        order_total
        sub_total
        start_date
        end_date
        orderLines { product_name product_code quantity row_total }
      }
    }
  }
}
"#;

/// A tokenised download URL for an online order's invoice.
///
/// There is an `e_receiptPdf` beside this one for in-store receipts. It is not
/// here: on both fascias it answers `success: false` with a raw PHP warning
/// naming a file on their server, so it has never worked. The line items in
/// [`IN_STORE_ORDERS`] are the receipt's contents anyway.
pub const INVOICE_PDF: &str = r#"
query InvoicePdf($invoiceId: String!) {
  customer { invoicePdf(invoice_id: $invoiceId) { success pdf_url message } }
}
"#;

// ---------------------------------------------------------------------- loyalty

/// Progress toward the next reward, and any vouchers already earned.
///
/// Per fascia and not shared: Briscoes Club and the Rebel Sport programme are
/// separate schemes with separate balances, which is the account half of the
/// same split that gives the two fascias separate logins.
pub const LOYALTY: &str = r#"
query GetLoyaltyDashboard($forceRefresh: Boolean = false) {
  customer {
    is_loyalty_eligible
    customer_type_code
    loyalty_info(force_refresh: $forceRefresh) {
      is_loyalty_member
      customer_type_code
      balance { current_spend spend_threshold spend_to_next_reward progress_percentage }
      available_vouchers { voucher_id voucher_code name title valid_until is_applied }
    }
  }
  storeConfig { loyalty_my_account_description }
}
"#;
