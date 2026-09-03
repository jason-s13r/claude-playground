//! `gsnz` -- New World, PAK'nSAVE and Woolworths from one command line.
//!
//! The interesting code is in `packages/`: the domain in `gsnz-core`, the two
//! protocols in `fsnz-api` and `wwnz-api`, the rendering in `cli-kit` and
//! `gsnz-ui`. What is left here is the part that is genuinely about this
//! program -- reading the environment once, resolving flags against config,
//! and turning a failure into an exit code.

mod app;
mod build;
mod cli;
mod commands;
mod config;
mod env;
mod error;
mod retailers;

use std::process::ExitCode;

use clap::Parser;

use crate::app::App;
use crate::cli::Cli;
use crate::error::AppResult;

fn main() -> ExitCode {
    let cli = Cli::parse();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("gsnz: could not start the async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.silent() => ExitCode::from(e.exit_code()),
        Err(e) => {
            eprintln!("gsnz: {e}");
            // The chain, not just the top: "reading the cart" alone says
            // nothing, and the cause underneath it is the part worth reading.
            // Skipping anything the line above already said, because a wrapper
            // carrying its source's own words would print them twice and read
            // as two problems.
            let mut shown = e.to_string();
            let mut cause = std::error::Error::source(&e);
            while let Some(e) = cause {
                let text = e.to_string();
                if !shown.contains(&text) {
                    eprintln!("      {text}");
                    shown = text;
                }
                cause = e.source();
            }
            if let Some(hint) = e.hint() {
                eprintln!("      {hint}");
            }
            // 2 misuse, 3 auth, 4 unsupported, 5 no store -- so a script can
            // tell them apart without reading this text.
            ExitCode::from(e.exit_code())
        }
    }
}

async fn run(cli: Cli) -> AppResult<()> {
    let app = App::new(&cli)?;
    commands::run(&app, cli.command).await
}
