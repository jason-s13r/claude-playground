//! `fsnz auth` -- logging in to Club Plus, logging out, and inspecting the
//! session and the tokens derived from it.

use anyhow::{bail, Context, Result};
use std::time::Duration;

use crate::app::App;
use crate::auth;
use crate::banner::Banner;
use crate::cli::AuthCommand;
use crate::commands::io::{human_duration, print_json, prompt, prompt_or_stdin};
use crate::token::{self, GuestToken};

/// Returns false when the command should exit non-zero.
pub async fn run(app: &App, cmd: &AuthCommand) -> Result<bool> {
    match cmd {
        AuthCommand::Login {
            email,
            password_command,
            no_store_password,
        } => {
            login(
                app,
                email.as_deref(),
                password_command.as_deref(),
                !no_store_password,
            )
            .await?;
            Ok(true)
        }
        AuthCommand::Logout => {
            logout(app)?;
            Ok(true)
        }
        AuthCommand::Refresh => {
            refresh(app).await?;
            Ok(true)
        }
        AuthCommand::Status => status(app).await,
    }
}

/// The banners an `auth` subcommand acts on.
///
/// One account covers both banners, so these commands are only useful across
/// both by default. `-b`/`FSNZ_BANNER` narrows them to the one named.
fn targets(app: &App) -> Vec<Banner> {
    match app.banner_flag {
        Some(b) => vec![b],
        None => Banner::ALL.to_vec(),
    }
}

/// Report the Club Plus session and each banner's token, without minting
/// anything: this reads the credential store and the token cache only, so it
/// stays instant and cannot itself change what it is describing.
///
/// Returns false when there is no session a command could actually use.
async fn status(app: &App) -> Result<bool> {
    let stored = auth::load(&app.secrets)?;
    let banners: Vec<(Banner, Option<GuestToken>)> = targets(app)
        .into_iter()
        .map(|b| (b, token::peek_cache(&app.paths, b)))
        .collect();

    // Reported because it is the only account-shaped thing the session says
    // about itself. It does not predict whether a banner will work -- see
    // `auth::linked_banners`.
    let linked: Vec<String> = stored
        .as_ref()
        .map(|s| auth::linked_banners(&s.access_token))
        .unwrap_or_default();
    // A password in the store (or a command that prints one) outlives the
    // refresh token, so it is what says whether a lapsed session can come back
    // without someone at the keyboard.
    let renewal =
        auth::password::Source::resolve(app.config.password_command.as_deref(), &app.secrets)?;
    let usable = stored
        .as_ref()
        .is_some_and(|s| s.is_fresh() || s.can_renew() || renewal.is_some());

    if app.json {
        print_json(&serde_json::json!({
            "logged_in": stored.is_some(),
            "usable": usable,
            "email": stored.as_ref().map(|s| s.email.clone()),
            "credential_store": app.secrets.backend().describe(),
            "unattended_renewal": renewal.as_ref().map(|r| r.describe()),
            "session": stored.as_ref().map(|s| serde_json::json!({
                "expires_in_seconds": remaining_secs(s.expires_at_ms()),
                "fresh": s.is_fresh(),
                "can_renew": s.can_renew() || renewal.is_some(),
                "banner_claim": auth::banner_claim(&s.access_token),
                "linked_banners": linked,
                "last_renewed_ms_ago": s.refreshed_at_ms.map(|t| token::now_ms().saturating_sub(t)),
            })),
            "banners": banners.iter().map(|(b, tok)| (b.id().to_string(), serde_json::json!({
                "cached": tok.is_some(),
                // The only way to read a token's value out of this tool. Human
                // output never prints it; this is read by scripts feeding
                // --token/FSNZ_TOKEN or poking the API directly.
                "token": tok.as_ref().map(|t| t.token.clone()),
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
                match (s.can_renew(), &renewal) {
                    (true, Some(r)) =>
                        format!("automatic, from the refresh token then {}", r.describe()),
                    (true, None) => "automatic, from the stored refresh token".to_string(),
                    (false, Some(r)) => format!("automatic, from {}", r.describe()),
                    (false, None) =>
                        "unavailable; log in again when the session expires".to_string(),
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

/// Discard the cached token for each banner in scope and mint a replacement.
///
/// Reports what it minted rather than the token itself; `auth status --json`
/// carries the value for anything that needs to read it.
///
/// One banner failing does not stop the other: they mint independently, and a
/// half-refreshed pair is more useful than an aborted one. Erroring only when
/// every banner failed keeps `auth refresh` honest as an exit-code check.
async fn refresh(app: &App) -> Result<()> {
    let mut results = Vec::new();
    for banner in targets(app) {
        let outcome = app.client(banner, true, true).await.map(|(_, guest)| guest);
        results.push((banner, outcome));
    }

    if app.json {
        print_json(&serde_json::json!({
            "banners": results.iter().map(|(b, r)| (b.id().to_string(), serde_json::json!({
                "ok": r.is_ok(),
                "source": r.as_ref().ok().map(|g| g.source.describe()),
                "expires_at_ms": r.as_ref().ok().map(|g| g.expires_at_ms),
                "expires_in_seconds": r.as_ref().ok().and_then(|g| g.expires_in()).map(|d| d.as_secs()),
                "error": r.as_ref().err().map(|e| format!("{e:#}")),
            }))).collect::<serde_json::Map<_, _>>(),
        }));
    } else {
        for (banner, result) in &results {
            match result {
                Ok(guest) => {
                    let expiry = match guest.expires_in() {
                        Some(d) => format!("expires in {}", human_duration(d)),
                        None => "already expired; the API will reject it".to_string(),
                    };
                    println!(
                        "  {:<10} token {}, {expiry}",
                        banner.name(),
                        guest.source.describe()
                    );
                }
                Err(e) => println!("  {:<10} FAILED: {e:#}", banner.name()),
            }
        }
    }

    if results.iter().all(|(_, r)| r.is_err()) {
        // The per-banner reasons are the only diagnosis available, and the
        // loop above has already printed them only in the human path.
        let reasons: Vec<String> = results
            .iter()
            .filter_map(|(b, r)| r.as_ref().err().map(|e| format!("{}: {e:#}", b.name())))
            .collect();
        bail!("no banner could mint a token. {}", reasons.join("; "));
    }
    Ok(())
}

/// Log in through Club Plus and confirm the session works at both banners.
async fn login(
    app: &App,
    email: Option<&str>,
    password_command: Option<&str>,
    store_password: bool,
) -> Result<()> {
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
        None => {
            rpassword::prompt_password("Club Plus password: ").context("reading the password")?
        }
    };
    if password.trim().is_empty() {
        bail!("no password given");
    }
    // Not conditional on where the password came from: `--password-command` is
    // the only way to log in without a terminal, so refusing to store what it
    // printed would mean a headless box could never set this up.
    let store_password = store_password && app.config.store_password.unwrap_or(true);

    let device_id = auth::device_id(&app.paths)?;
    let session = match auth::login(&app.http, &email, &password, &device_id).await? {
        auth::Login::Complete(session) => session,
        // Password was right; a code is already sent. Nothing stored until it
        // comes back.
        auth::Login::ChallengeRequired(challenge) => {
            // stderr: on stdout this would land in front of `--json` output.
            eprintln!(
                "Club Plus wants to verify this device and has sent a code to {email} ({}).",
                challenge.method
            );
            let code = prompt_or_stdin("Verification code: ")?;
            auth::complete_challenge(&app.http, &challenge, &code).await?
        }
    };
    auth::save(
        &app.secrets,
        &auth::StoredLogin {
            email: email.clone(),
            access_token: session.access_token.clone(),
            refresh_token: session.refresh_token.clone(),
            refreshed_at_ms: Some(token::now_ms()),
        },
    )?;
    // Kept so `active_session` can sign in again once the refresh token is
    // spent, which is what lets an unattended run outlive one session.
    if store_password {
        auth::password::save(&app.secrets, &password)?;
    } else {
        auth::password::clear(&app.secrets)?;
    }

    // Prove the session actually mints banner tokens before calling it a
    // success -- a stored login that does not work is worse than none.
    let mut results = Vec::new();
    for banner in targets(app) {
        let endpoints = banner.endpoints();
        let outcome = auth::banner_token(&app.http, banner, &endpoints, &session, &device_id).await;
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
            "password_stored": store_password,
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
        if store_password {
            println!(
                "Password kept in {} as well, so the session signs itself in again\n\
                 once the refresh token runs out. `--no-store-password` skips that.",
                app.secrets.backend().describe()
            );
        } else if command.is_some() {
            println!("Password not stored; password_command renews the session instead.");
        } else {
            println!(
                "Password not stored; renewal stops once the refresh token lapses.\n\
                 Set password_command, or log in without --no-store-password."
            );
        }
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
/// Unlike the rest of `auth`, this ignores `-b`: there is one Club Plus
/// session behind both banners, so there is no such thing as logging out of
/// one of them. It clears the session and every cached token.
fn logout(app: &App) -> Result<()> {
    let had_login = auth::clear(&app.secrets)?;
    let had_password = auth::password::clear(&app.secrets)?;
    // The kept cookies include `fs-user-token` and `refresh_token`.
    let _ = crate::cookies::Jar::clear(&app.secrets);
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
            "had_password": had_password,
            "cleared_token_caches": cleared,
        }));
    } else if had_login {
        println!("Logged out; stored login removed.");
    } else {
        println!("No stored login.");
    }
    Ok(())
}
