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

pub use cart::CartView;
pub use compare::CompareTable;
pub use departments::DepartmentTree;
pub use orders::{OrderDetail, OrderList};
pub use products::{price_label, unit_label, ProductList};
pub use stores::StoreList;
