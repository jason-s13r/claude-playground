//! wwnz -- unofficial CLI for Woolworths New Zealand.
//!
//! Not affiliated with Woolworths. It calls the same undocumented GraphQL
//! endpoint their website calls, which can change without notice.

mod api;
mod app;
mod auth;
mod build;
mod cli;
mod commands;
mod config;
mod domain;
mod output;
mod secrets;
mod session;
mod update;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::process::ExitCode;
use std::time::Duration;

use api::Endpoints;
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

    // A bare `wwnz` is someone looking for the commands. Answer before touching
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

    let mut app = App {
        secrets: Secrets::new(paths.state_dir.clone()),
        paths,
        config,
        endpoints: Endpoints::resolve(),
        http: http_client()?,
        json: cli.json,
        store_flag: cli.store.clone(),
    };

    commands::dispatch(&mut app, command).await
}

/// The client every request goes through.
///
/// `wreq` -- `reqwest` with a browser TLS handshake -- and deliberately not
/// plain `reqwest`. Akamai sits in front of woolworths.co.nz and scores the
/// handshake: with rustls the storefront withholds its bot-manager cookies and
/// the login is refused with a bare 400; with a Firefox handshake the same
/// requests are answered normally and the cookies arrive.
///
/// The sibling `foodstuffs-nz-cli` shells out to curl for the same class of
/// problem, because Cloudflare accepts OpenSSL handshakes. Do not copy that
/// here: the two vendors score differently and curl fares worse than rustls
/// against this one.
///
/// Nor curl, for the same reason in the other direction: asked for the home
/// page, curl is issued neither `__guest__token` nor `ak_bmsc`. A browser
/// handshake is issued both.
fn http_client() -> Result<wreq::Client> {
    Ok(wreq::Client::builder()
        // The whole point. This sets the TLS handshake, the HTTP/2 settings and
        // the headers together, which is what Akamai scores.
        .emulation(session::EMULATION)
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()?)
}
