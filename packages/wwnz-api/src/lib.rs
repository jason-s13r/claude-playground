//! The Woolworths NZ GraphQL API.
//!
//! One endpoint, `POST /api/graphql`, answers everything the website does:
//! search, browse, specials, stores, the cart and order history. It is
//! undocumented and reverse-engineered from the site's own traffic, so
//! everything arrives optional -- a field Woolworths renames should degrade to
//! a missing column, not a failed command.
//!
//! Authorisation is entirely by cookie. The guest token covers products and
//! stores; the cart and orders need an account, and a Woolworths session cannot
//! be refreshed -- the cookie is encrypted and only the site can mint one, so
//! the only renewal is walking the whole login flow again.
//!
//! This crate speaks its own vendor-shaped types and does not depend on a
//! shared domain crate.

pub mod auth;
mod client;
mod domain;
mod endpoints;
mod error;
pub mod gql;
pub mod http;
pub mod session;
mod wire;

pub use client::{Client, Reauth, SearchBy, SearchResult, DEFAULT_SORT, SORTS};
pub use domain::{
    format_quantity, variant_key, Cart, CartLine, Category, Change, Fee, Filter, Order,
    OrderDetail, OrderLineItem, OrderPage, Product, Store,
};
pub use endpoints::Endpoints;
pub use error::{Error, Result};
pub use http::{client_spec, EMULATION};
pub use session::{Session, StoredSession};
