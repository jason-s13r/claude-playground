//! The grocery domain, with no idea how any supermarket answers a request.
//!
//! Two apps already speak to New Zealand's supermarkets, each with its own
//! shape for a product and a cart. This crate is the vocabulary a combined CLI
//! needs: one `Product`, one `Cart`, one `Quantity`, and a [`Retailer`] trait
//! that a per-vendor adapter implements.
//!
//! Nothing here does I/O. The dependency list is `serde`, `thiserror` and the
//! attribute macro for async traits -- that is the point, and it is what keeps
//! the domain reusable by something that is not a CLI.

pub mod cart;
pub mod compare;
pub mod department;
pub mod error;
pub mod money;
pub mod order;
pub mod product;
pub mod retailer;
pub mod search;
pub mod store;

pub use cart::{Adjustment, Cart, CartLine, Change, Quantity};
pub use compare::{pair, MatchKey, Row};
pub use department::Department;
pub use error::{Error, Result};
pub use money::dollars;
pub use order::{Order, OrderFilter, OrderLine, OrderSummary};
pub use product::{Product, SaleUnit};
pub use retailer::{AuthStatus, Caps, Retailer, RetailerId};
pub use search::{Search, SearchBy, SearchResult, Sort};
pub use store::{Store, StoreRef};
