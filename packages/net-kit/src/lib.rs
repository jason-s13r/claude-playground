//! The process boundary: HTTP that is not scored as a bot, cookies that
//! survive a run, credentials that are not a plaintext file, and the paths
//! those live under.
//!
//! One rule holds this crate together: **nothing here reads the environment.**
//! The existing CLIs call `std::env::var` from inside `Endpoints::resolve` and
//! `Secrets::new`, which is why several of their unit tests have to `set_var`
//! and race each other. Every entry point below takes the value instead. The
//! app reads the environment once, at the top, and passes the results down --
//! and that is also what lets two apps with different variable prefixes share
//! this code. `clippy.toml` enforces it.

pub mod config;
pub mod cookies;
pub mod error;
pub mod http;
pub mod jwt;
pub mod password;
pub mod paths;
pub mod run;
pub mod secrets;

pub use cookies::Jar;
pub use error::{AuthFault, Error, Fault, HttpError, Result};
pub use http::ClientSpec;
pub use paths::{restrict, Paths};
pub use secrets::{Backend, Secrets};

/// Re-exported so every consumer compiles against one `wreq`.
///
/// Five crates in this repo name `wreq::Client` in their public signatures and
/// each has its own lockfile. Two of them resolving different majors would
/// surface as a baffling trait mismatch rather than a version error, so they
/// depend on `net_kit::wreq` rather than listing `wreq` themselves.
pub use wreq;
pub use wreq_util;
