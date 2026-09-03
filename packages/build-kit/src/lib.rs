//! What a binary is, where it came from, and how it replaces itself.
//!
//! Two halves, because they run at different times and want different
//! dependencies:
//!
//! - [`emit`], used from a consumer's `build.rs`, stamps provenance in. `std`
//!   only.
//! - the rest, used at runtime, reads that stamp back and installs newer
//!   releases.
//!
//! # The `env!` problem
//!
//! `env!` expands in the crate where it is *written*, and `cargo:rustc-env`
//! set by an app's build script is only visible while compiling that app. So
//! this crate can never `env!("GSNZ_VERSION")` on a consumer's behalf.
//!
//! Instead [`emit::Stamper`] writes a Rust source file into the consumer's
//! `OUT_DIR` and the consumer includes it:
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     build_kit::emit::Stamper::new("GSNZ")
//!         .tag_glob("grocery-nz-cli/v*")
//!         .emit()
//!         .expect("stamping the build");
//! }
//!
//! // src/build.rs
//! include!(concat!(env!("OUT_DIR"), "/build_stamp.rs"));  // defines STAMP
//! ```
//!
//! `OUT_DIR` is per-crate, so the `env!` resolves in the right place. The
//! payoff beyond correctness: [`Stamp`] is an ordinary struct, so version
//! strings and dates are unit-testable without compiling anything.

/// This crate's own version, for a consumer that reports what it was
/// built against.
///
/// `env!` expands where it is written, so this is the one place it can be
/// read from: a consumer writing `env!("CARGO_PKG_VERSION")` would get its
/// own version back, not this one.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(feature = "emit")]
pub mod emit;

mod date;
mod stamp;

pub use date::{iso_date, now};
pub use stamp::Stamp;

#[cfg(feature = "runtime")]
mod error;
#[cfg(feature = "runtime")]
mod install;
#[cfg(feature = "runtime")]
pub mod update;

#[cfg(feature = "runtime")]
pub use error::{Error, Result};
#[cfg(feature = "runtime")]
pub use install::{exe_path, Install};
