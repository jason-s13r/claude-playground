//! The Briscoe Group New Zealand storefronts: Briscoes and Rebel Sport.
//!
//! **One backend, two fascias.** Both sites are a single Magento 2 deployment
//! with a PWA Studio frontend, and which catalogue answers is decided by a
//! `store` request header. That is why this is one crate with a [`Banner`]
//! parameter rather than two crates: the schema, the operation names and the
//! host pool are shared, and only the catalogue, the search index and the
//! identity site differ.
//!
//! It is *not* the Foodstuffs arrangement, despite looking like it. New World
//! and PAK'nSAVE share one Club Plus login across two banners; these two share
//! a backend but have **separate Gigya sites with separate API keys**, so
//! signing in to Briscoes does not sign in to Rebel Sport, and a banner's
//! credentials are filed apart. Nor is there anything to compare between them:
//! the SKU namespaces are disjoint and so are the catalogues.
//!
//! Three surfaces, and they need different things:
//!
//! * **Klevu** answers search and browse, needs no credentials, and is reached
//!   with a public API key the storefront itself publishes. See [`search`].
//! * **GraphQL** answers everything else. Reads need no account; the cart,
//!   wishlist, orders and loyalty need a customer token.
//! * A small **REST** service answers click-and-collect stock, with a stricter
//!   validator than anything else here. See [`stock`].
//!
//! Signing in is the one hard part, and it is hard in exactly one place:
//! Gigya's `accounts.login` refuses any request without a reCAPTCHA token, and
//! refuses it before checking the password. Everything after that -- including
//! minting a fresh customer token every two hours, forever -- is unguarded.
//! [`auth`] is built around that split.
//!
//! Everything arrives loosely typed. Magento sends booleans as integers and
//! identifiers as numbers, Klevu sends every price as a string, and a store's
//! opening hours arrive as a JSON document nested inside a JSON string. So a
//! field either vendor renames should cost a column, not a command: see
//! [`wire`].
//!
//! This crate speaks its own vendor-shaped types and does not depend on a
//! shared domain crate.

/// This crate's own version, for a consumer that reports what it was
/// built against.
///
/// `env!` expands where it is written, so this is the one place it can be
/// read from: a consumer writing `env!("CARGO_PKG_VERSION")` would get its
/// own version back, not this one.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod auth;
mod banner;
mod client;
mod domain;
mod error;
pub mod gql;
pub mod http;
pub mod search;
pub mod session;
pub mod stock;
pub mod wire;

pub use banner::{Banner, Endpoints};
pub use client::{Client, SessionStore};
pub use domain::{
    Attribute, Cart, CartLine, Category, Customer, Discount, Facet, FacetOption, Fulfilment,
    GigyaConfig, Invoice, Listing, Loyalty, Money, OpeningHours, Order, OrderDetail, OrderLine,
    OrderPage, Product, ProductDetail, Receipt, ReceiptLine, ReceiptPage, Shipment, Stock, Store,
    Storefront, Variant, Voucher, Wishlist, WishlistItem,
};
pub use error::{Error, Result};
pub use http::{client_spec, client_spec_for, profile, EMULATION};
pub use search::{Query, DEFAULT_SORT, PAGE_SIZE, SORTS};
pub use session::{Session, StoredSession};
