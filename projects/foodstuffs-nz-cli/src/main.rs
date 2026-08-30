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
mod domain;
mod output;
mod process;
mod secrets;
mod token;
mod update;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::process::ExitCode;
use std::time::Duration;

use app::App;
use cli::Cli;
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

    let paths = Paths::resolve()?;
    let config = Config::load(&paths)?;
    let banner = match cli.banner {
        Some(b) => b,
        None => config.default_banner()?,
    };

    let mut app = App {
        secrets: Secrets::new(paths.state_dir.clone()),
        paths,
        config,
        http: http_client()?,
        json: cli.json,
        store_flag: cli.store.clone(),
        token_flag: cli.token.clone(),
        banner_flag: cli.banner,
    };

    commands::dispatch(&mut app, banner, command).await
}

fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        // HTTP/1.1 throughout, deliberately. Cloudflare fingerprints the HTTP/2
        // connection settings in front of both the storefronts and the Club Plus
        // API, and answers anything it does not recognise with a 403 challenge
        // page that no combination of headers gets past. Over HTTP/1.1 every one
        // of those hosts answers normally.
        .http1_only()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()?)
}
