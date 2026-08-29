//! `fsnz auth` -- logging in to Club Plus, logging out, and inspecting the
//! session and the tokens derived from it.

use anyhow::{bail, Context, Result};
use std::time::Duration;

use crate::app::App;
use crate::auth;
use crate::banner::Banner;
use crate::cli::AuthCommand;
use crate::commands::io::{human_duration, print_json, prompt};
use crate::token::{self, GuestToken};

/// Returns false when the command should exit non-zero.
pub async fn run(app: &App, banner: Banner, cmd: &AuthCommand) -> Result<bool> {
    match cmd {
        AuthCommand::Login {
            email,
            password_command,
        } => {
            login(app, email.as_deref(), password_command.as_deref()).await?;
            Ok(true)
        }
        AuthCommand::Logout => {
            logout(app)?;
            Ok(true)
        }
        AuthCommand::Token { refresh, raw } => {
            show_token(app, banner, *refresh, *raw).await?;
            Ok(true)
        }
        AuthCommand::Status => status(app).await,
    }
}

/// Report the Club Plus session and each banner's token, without minting
/// anything: this reads the credential store and the token cache only, so it
/// stays instant and cannot itself change what it is describing.
///
/// Returns false when there is no session a command could actually use.
async fn status(app: &App) -> Result<bool> {
    let stored = auth::load(&app.secrets)?;
    let banners: Vec<(Banner, Option<GuestToken>)> = Banner::ALL
        .iter()
        .map(|b| (*b, token::peek_cache(&app.paths, *b)))
        .collect();

    // Reported because it is the only account-shaped thing the session says
    // about itself. It does not predict whether a banner will work -- see
    // `auth::linked_banners`.
    let linked: Vec<String> = stored
        .as_ref()
        .map(|s| auth::linked_banners(&s.access_token))
        .unwrap_or_default();
    let usable = stored
        .as_ref()
        .is_some_and(|s| s.is_fresh() || s.can_renew());

    if app.json {
        print_json(&serde_json::json!({
            "logged_in": stored.is_some(),
            "usable": usable,
            "email": stored.as_ref().map(|s| s.email.clone()),
            "credential_store": app.secrets.backend().describe(),
            "session": stored.as_ref().map(|s| serde_json::json!({
                "expires_in_seconds": remaining_secs(s.expires_at_ms()),
                "fresh": s.is_fresh(),
                "can_renew": s.can_renew(),
                "banner_claim": auth::banner_claim(&s.access_token),
                "linked_banners": linked,
                "last_renewed_ms_ago": s.refreshed_at_ms.map(|t| token::now_ms().saturating_sub(t)),
            })),
            "banners": banners.iter().map(|(b, tok)| (b.id().to_string(), serde_json::json!({
                "cached": tok.is_some(),
                "expires_in_seconds": tok.as_ref().and_then(|t| remaining_secs(Some(t.expires_at_ms))),
                "banner_claim": tok.as_ref().and_then(|t| auth::banner_claim(&t.token)),
                "linked": linked.iter().any(|l| l == b.code()),
            }))).collect::<serde_json::Map<_, _>>(),
        }));
        return Ok(usable);
    }

    println!("Club Plus");
    match &stored {
        None => println!("  session      none; run `fsnz auth login`"),
        Some(s) => {
            println!("  account      {}", s.email);
            println!("  stored in    {}", app.secrets.backend().describe());
            match remaining_secs(s.expires_at_ms()) {
                Some(secs) => println!(
                    "  session      valid for {}",
                    human_duration(Duration::from_secs(secs))
                ),
                None if s.can_renew() => println!("  session      expired; renews on next use"),
                None => println!("  session      expired"),
            }
            println!(
                "  renewal      {}",
                if s.can_renew() {
                    "automatic, from the stored refresh token"
                } else {
                    "unavailable; log in again when the session expires"
                }
            );
            if let Some(ago) = s.refreshed_at_ms.map(|t| token::now_ms().saturating_sub(t)) {
                println!(
                    "  renewed      {} ago",
                    human_duration(Duration::from_millis(ago))
                );
            }
            println!(
                "  linked to    {}",
                if linked.is_empty() {
                    "unknown".to_string()
                } else {
                    linked.join(", ")
                }
            );
        }
    }

    for (banner, cached) in &banners {
        println!("\n{}", banner.name());
        match cached {
            Some(t) => {
                match remaining_secs(Some(t.expires_at_ms)) {
                    Some(secs) => println!(
                        "  token        cached, expires in {}",
                        human_duration(Duration::from_secs(secs))
                    ),
                    None => println!("  token        cached but expired; re-minted on next use"),
                }
                match auth::banner_claim(&t.token).as_deref() {
                    Some(code) if code == banner.code() => {
                        println!("  scope        {code}; cart available")
                    }
                    // The failure this tool exists to make visible: a national
                    // token is accepted by the cart and answers it with an
                    // empty cart belonging to nobody.
                    Some(code) => println!(
                        "  scope        {code}, not {}; the cart will read as empty",
                        banner.code()
                    ),
                    None => println!("  scope        unknown"),
                }
            }
            None => println!("  token        none cached; minted on next use"),
        }
        if stored.is_some() && !linked.is_empty() {
            println!(
                "  linked       {}",
                if linked.iter().any(|l| l == banner.code()) {
                    "yes"
                } else {
                    "no"
                }
            );
        }
    }

    Ok(usable)
}

/// Whole seconds until a moment in the future, or None once it has passed.
fn remaining_secs(expires_at_ms: Option<u64>) -> Option<u64> {
    let expires_at_ms = expires_at_ms?;
    let now = token::now_ms();
    (expires_at_ms > now).then(|| (expires_at_ms - now) / 1000)
}

async fn show_token(app: &App, banner: Banner, refresh: bool, raw: bool) -> Result<()> {
    let guest = app.client(banner, refresh, true).await?.1;
    if raw {
        println!("{}", guest.token);
        return Ok(());
    }
    if app.json {
        print_json(&serde_json::json!({
            "banner": banner.id(),
            "token": guest.token,
            "source": guest.source.describe(),
            "expires_at_ms": guest.expires_at_ms,
            "expires_in_seconds": guest.expires_in().map(|d| d.as_secs()),
        }));
        return Ok(());
    }
    println!("{}: token {}", banner.name(), guest.source.describe());
    match guest.expires_in() {
        Some(d) => println!("expires in {}", human_duration(d)),
        None => println!("expired; run `fsnz auth token --refresh`"),
    }
    println!("{}", guest.token);
    Ok(())
}

/// Log in through Club Plus and confirm the session works at both banners.
async fn login(app: &App, email: Option<&str>, password_command: Option<&str>) -> Result<()> {
    let email = match email.map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) => e.to_string(),
        None => match auth::load(&app.secrets)?.map(|s| s.email) {
            Some(e) => {
                println!("Logging in as {e}");
                e
            }
            None => prompt("Club Plus email: ")?,
        },
    };

    let command = password_command
        .map(str::to_string)
        .or_else(|| app.config.password_command.clone())
        .filter(|c| !c.trim().is_empty());
    let password = match &command {
        Some(cmd) => crate::process::run::capturing(cmd).await?,
        None => rpassword::prompt_password("Club Plus password (not stored): ")
            .context("reading the password")?,
    };
    if password.trim().is_empty() {
        bail!("no password given");
    }

    let device_id = auth::device_id(&app.paths)?;
    let session = auth::login(&email, &password, &device_id).await?;
    auth::save(
        &app.secrets,
        &auth::StoredLogin {
            email: email.clone(),
            access_token: session.access_token.clone(),
            refresh_token: session.refresh_token.clone(),
            refreshed_at_ms: Some(token::now_ms()),
        },
    )?;

    // Prove the session actually mints banner tokens before calling it a
    // success -- a stored login that does not work is worse than none.
    let mut results = Vec::new();
    for banner in Banner::ALL {
        let endpoints = banner.endpoints();
        let outcome = auth::banner_token(banner, &endpoints, &session, &device_id).await;
        // Keep what was just proved. Throwing it away would mean the very next
        // command mints a second token for no reason.
        if let Ok(minted) = &outcome {
            token::cache_account_token(&app.paths, banner, minted);
        }
        results.push((banner, outcome));
    }

    if app.json {
        print_json(&serde_json::json!({
            "email": email,
            "stored_in": app.secrets.backend().describe(),
            "banners": results.iter().map(|(b, r)| serde_json::json!({
                "banner": b.id(),
                "ok": r.is_ok(),
                "error": r.as_ref().err().map(|e| format!("{e:#}")),
            })).collect::<Vec<_>>(),
        }));
    } else {
        println!(
            "Logged in as {email}; session kept in {}.",
            app.secrets.backend().describe()
        );
        for (banner, result) in &results {
            match result {
                Ok(_) => println!("  {:<10} token minted", banner.name()),
                Err(e) => println!("  {:<10} FAILED: {e:#}", banner.name()),
            }
        }
        println!("Password not stored. Set password_command to skip the prompt.");
    }

    if results.iter().all(|(_, r)| r.is_err()) {
        // Carry the per-banner reasons into the error. They hold the only
        // detail that makes this fixable, and an aggregate that drops them
        // leaves the user with nothing to report.
        let detail = results
            .iter()
            .filter_map(|(b, r)| r.as_ref().err().map(|e| format!("  {}: {e:#}", b.name())))
            .collect::<Vec<_>>()
            .join("\n");
        bail!("logged in to Club Plus, but neither banner would issue a token:\n{detail}");
    }
    Ok(())
}

/// Forget the login and every cached token derived from it.
fn logout(app: &App) -> Result<()> {
    let had_login = auth::clear(&app.secrets)?;
    let mut cleared = Vec::new();
    for banner in Banner::ALL {
        let file = app.paths.token_file(banner);
        if std::fs::remove_file(&file).is_ok() {
            cleared.push(banner.id());
        }
    }
    if app.json {
        print_json(&serde_json::json!({
            "had_login": had_login,
            "cleared_token_caches": cleared,
        }));
    } else if had_login {
        println!("Logged out; stored login removed.");
    } else {
        println!("No stored login.");
    }
    Ok(())
}
