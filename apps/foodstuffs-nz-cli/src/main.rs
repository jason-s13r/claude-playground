//! fsnz -- unofficial CLI for New World and PAK'nSAVE (Foodstuffs NZ).
//!
//! Not affiliated with Foodstuffs. It calls the same undocumented endpoints
//! their websites call, which can change without notice.

mod api;
mod app;
mod auth;
mod banner;
mod build;
mod cli;
mod commands;
mod config;
mod cookies;
mod domain;
mod http;
mod output;
mod process;
mod secrets;
mod token;
mod update;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::process::ExitCode;
use std::sync::Arc;

use app::App;
use cli::{Cli, Command};
use config::{Config, Paths};
use secrets::Secrets;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    // A bare `fsnz` is someone looking for the commands. Answer before touching
    // config, so a broken config file still gets help rather than an error.
    let Some(command) = &cli.command else {
        Cli::command().print_long_help()?;
        return Ok(ExitCode::SUCCESS);
    };

    // Completion scripts describe the command surface and nothing else, so
    // they are generated without config, credentials or a network.
    if let Command::Completions { shell } = command {
        commands::completions::run(*shell)?;
        return Ok(ExitCode::SUCCESS);
    }

    let paths = Paths::resolve()?;
    let config = Config::load(&paths)?;
    let banner = match cli.banner {
        Some(b) => b,
        None => config.default_banner()?,
    };

    let secrets = Secrets::new(paths.state_dir.clone());
    let jar = Arc::new(cookies::Jar::load(&secrets));

    let mut app = App {
        secrets,
        paths,
        config,
        http: http::client(jar.clone())?,
        json: cli.json,
        store_flag: cli.store.clone(),
        token_flag: cli.token.clone(),
        banner_flag: cli.banner,
    };

    let result = commands::dispatch(&mut app, banner, command).await;
    // Either way: a failed run may still have been handed a good cookie.
    jar.save(&app.secrets);
    result
}
