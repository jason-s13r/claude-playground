//! The Foodstuffs NZ edge API: New World and PAK'nSAVE.
//!
//! Both banners are one Foodstuffs platform wearing two names, so one client
//! drives both -- they differ in which hostnames they answer on and which code
//! their tokens are scoped to.
//!
//! Everything here is reverse-engineered from what the websites' own frontends
//! call, so every field arrives optional on purpose: a field Foodstuffs renames
//! should degrade to a missing column, not a failed command.
//!
//! This crate speaks its own vendor-shaped types and does not depend on any
//! shared domain crate. Converting is the caller's job, which is what keeps it
//! usable on its own.

pub mod auth;
mod banner;
pub mod cart;
mod client;
mod domain;
mod error;
pub mod http;
pub mod order;
pub mod token;
mod wire;

pub use banner::{Banner, ClubPlusEndpoints, Endpoints};
pub use cart::{Cart, CartItem, Change, SaleType};
pub use client::{filters, Client, SearchResult, DEFAULT_SORT};
pub use domain::{Category, Product, Store};
pub use error::{Error, Result};
pub use http::{client_spec, cookie_keep, EMULATION};
pub use order::{Order, OrderLine, OrderPage, OrderSummary};
