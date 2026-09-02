//! What the rest of the tool works with, once a banner's response has been
//! normalised away from its wire shape.

pub mod cart;
pub mod compare;
pub mod money;
pub mod order;
pub mod product;
pub mod store;

pub use money::dollars;
pub use product::Product;
pub use store::Store;
