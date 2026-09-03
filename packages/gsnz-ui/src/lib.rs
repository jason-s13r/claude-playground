//! Showing groceries to a person.
//!
//! Every type here is a [`cli_kit::View`] over a [`gsnz_core`] type, which is
//! the whole shape of the crate: the domain knows nothing about rendering, the
//! rendering knows nothing about HTTP, and `--json` falls out of the same
//! struct the text renderer reads rather than being written twice.

mod cart;
mod compare;
mod departments;
mod orders;
mod products;
mod stores;

/// This crate's own version, for a consumer that reports what it was
/// built against.
///
/// `env!` expands where it is written, so this is the one place it can be
/// read from: a consumer writing `env!("CARGO_PKG_VERSION")` would get its
/// own version back, not this one.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use cart::CartView;
pub use compare::CompareTable;
pub use departments::DepartmentTree;
pub use orders::{OrderDetail, OrderList};
pub use products::{price_label, unit_label, ProductList};
pub use stores::StoreList;
