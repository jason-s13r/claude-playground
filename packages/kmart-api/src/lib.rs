//! Kmart, in Australia and New Zealand.
//!
//! Four surfaces behind one [`Client`], because Kmart's website is four
//! separate services stitched together in the browser:
//!
//! - a **GraphQL gateway** at `api.kmart.co.nz` for the cart, the wishlist,
//!   stock, store detail, postcodes and orders;
//! - **Constructor.io** at `ac.cnstrc.com` for everything that lists products
//!   -- search, browse, the category tree and its facets;
//! - the **Next.js storefront**, whose product pages carry the whole product
//!   record in a `__NEXT_DATA__` blob;
//! - **Auth0** at `auth.kmart.com.au`, which mints the bearer token the
//!   gateway wants.
//!
//! The two countries are one backend. The same schema answers on two hosts,
//! store ids share a namespace across the pair, and the login is a single
//! Auth0 tenant. Only the catalogue and the currency differ, so [`Country`] is
//! a parameter threaded through this crate rather than a build of its own.
//!
//! Two things shape the rest. A product's leaf id is its **keycode** --
//! `43165537`, which is Constructor's `variation_id`, the gateway's `sku` and
//! the last segment of the page slug, so one id works everywhere and there is
//! no id translation anywhere in this crate. And **stock is a function of a
//! postcode**, not of a store: the gateway is asked what is available near a
//! postcode and answers per channel -- home delivery, click and collect,
//! express -- so an item can be out of stock online and on a shelf a suburb
//! away.
//!
//! **The catalogue works cold; the gateway does not.** Constructor.io is a
//! third party and answers anyone. Kmart's own hosts sit behind Akamai Bot
//! Manager, which serves a sensor-script interstitial instead of the page and
//! a `429` carrying `cpr_chlge` instead of the query -- to any client, on any
//! TLS fingerprint, warmed up or not. Nothing here tries to solve that. The
//! way in is [`auth::from_netscape`]: cookies exported from a browser that has
//! already passed it, which is the same escape hatch `wwnz-api` documents.
//!
//! The login is guarded at one step, and only one: the password submit. So a
//! session cannot be obtained here either, and [`auth::refresh`] -- driven by
//! a refresh token lifted out of a browser -- is what an account runs on. See
//! [`auth`] for the step-by-step measurement.
//!
//! Everything arrives optional. Both JSON halves are undocumented and one of
//! them is a search index whose fields differ per category, so a field Kmart
//! renames should degrade to a missing column rather than a failed command.
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
pub mod catalog;
mod client;
pub mod country;
mod domain;
mod endpoints;
mod error;
pub mod gql;
pub mod http;
pub mod session;
pub mod vendor;
mod wire;

pub use catalog::{Query, Select, DEFAULT_SORT, PAGE_SIZE, SORTS};
pub use client::{Client, Reauth, SessionStore};
pub use country::Country;
pub use domain::{
    Availability, Cart, CartLine, Category, Customer, Facet, FacetOption, Listing, Order,
    OrderPage, Postcode, Price, Product, Rating, Sort, Store, StoreStock, Tracking, TradingDay,
    Variation, Wishlist, WishlistItem,
};
pub use endpoints::Endpoints;
pub use error::{Error, Result};
pub use http::{client_spec, EMULATION};
pub use session::{Session, StoredSession, Tokens};
pub use vendor::{Vendor, AUTH_AUDIENCE, AUTH_SCOPE};
