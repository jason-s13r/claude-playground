//! Presentation for a command line tool, knowing nothing about what is being
//! presented.
//!
//! This crate exists to keep a dependency *out*. Both existing CLIs decide
//! between human output and `--json` with an early-return `if` in every command
//! function, which means the two paths drift and neither is testable without
//! running the binary. Here there is one [`emit`] and a [`View`] per thing, and
//! a [`Out`] can be pointed at a buffer, so a renderer is an ordinary unit
//! test.
//!
//! There is no domain type in here and there must never be one: a table is a
//! table whether it holds groceries or anything else.

pub mod completions;
pub mod doctor;
pub mod io;
pub mod out;
pub mod table;

pub use doctor::{Check, Report, Status};
pub use io::{confirm, human_duration, prompt, prompt_or_stdin, prompt_password};
pub use out::{emit, Format, Out, View};
pub use table::{plural, qualified, table};

/// Re-exported so a consumer building rows compiles against the same
/// `comfy-table` this crate's [`table`] returns, and the same `serde_json`
/// whose `Value` appears in [`View::json`]. Two majors of either in one
/// dependency tree would surface as a baffling type mismatch rather than a
/// version error.
pub use comfy_table;
pub use serde_json;
