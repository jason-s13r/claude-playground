//! `auth` -- signing in, and the four ways a session ends.

use cli_kit::{emit, prompt, prompt_password, Out, View};
use gsnz_core::AuthStatus;
use serde::Serialize;
use std::io::Write;

use crate::app::{self, App};
use crate::cli::AuthAction;
use crate::error::{AppError, AppResult};

pub async fn run(app: &App, action: AuthAction) -> AppResult<()> {
    match action {
        AuthAction::Login {
            email,
            password_command,
            no_store_password,
        } => login(app, email, password_command, no_store_password).await,
        AuthAction::Import { file } => import(app, &file).await,
        AuthAction::Refresh => refresh(app).await,
        AuthAction::Status => status(app).await,
        AuthAction::Logout => logout(app).await,
    }
}

/// Sign in once per credential, not once per shop.
///
/// `gsnz auth login` with no `-b` is the whole of setting this tool up: two
/// prompts, because there are two accounts. Asking for the Club Plus password
/// twice -- once as New World and once as PAK'nSAVE -- is the thing this
/// avoids, and the thing the old per-shop version did not tell you about.
async fn login(
    app: &App,
    email: Option<String>,
    password_command: Option<String>,
    no_store_password: bool,
) -> AppResult<()> {
    let targets = app.auth_targets();
    if email.is_some() && targets.len() > 1 {
        return Err(AppError::usage(
            "--email names one account, but this would sign in to more than one: \
             name the shop too, as `gsnz -b ww auth login --email ...`",
        ));
    }

    let mut statuses = Vec::new();
    for target in targets {
        let covers = app::name_shops(&target.covers);
        // On stderr with the prompts: which account is being asked for is part
        // of the conversation, not part of the result.
        eprintln!("{covers}:");

        let handle = app.registry.get(target.through)?;
        let email = match &email {
            Some(email) => email.clone(),
            None => prompt("  Email")?,
        };
        // A configured command is the account's real source of truth where one
        // exists, so it beats both the prompt and anything stored.
        let password =
            match password_command
                .as_deref()
                .or(app.config.auth.password_command.as_deref())
            {
                Some(command) => net_kit::run::capturing("password_command", command).await?,
                None => prompt_password("  Password")?,
            };

        // Never echoed, never logged: the code goes straight back to Club Plus.
        let ask = |method: &str| {
            eprintln!("  A verification code was sent by {method}.");
            prompt("  Code")
        };
        handle.login(&email, &password, &ask).await?;

        // Only after the login worked: storing a password that does not sign
        // in is worse than storing none.
        if app.config.auth.store_password && !no_store_password {
            let secrets = app.secrets(target.through);
            if let Err(e) = net_kit::password::save(&secrets, &password) {
                eprintln!("gsnz: signed in, but the password could not be kept: {e}");
            }
        }

        // Every shop the credential covers, so a login that also signed in
        // PAK'nSAVE says so instead of leaving it to be discovered.
        statuses.extend(statuses_for(app, &target.covers).await);
    }
    show(app, statuses)
}

async fn refresh(app: &App) -> AppResult<()> {
    let mut statuses = Vec::new();
    for target in app.auth_targets() {
        app.registry.get(target.through)?.refresh_session().await?;
        statuses.extend(statuses_for(app, &target.covers).await);
    }
    show(app, statuses)
}

/// What each shop reports, with a failure reported rather than raised.
///
/// `auth status` is what someone runs to find out that a stored blob is
/// unreadable, so it must not be the command that fails because of one.
async fn statuses_for(app: &App, ids: &[gsnz_core::RetailerId]) -> Vec<AuthStatus> {
    let mut out = Vec::new();
    for &id in ids {
        let result = match app.registry.get(id) {
            Ok(handle) => handle.auth_status().await,
            Err(e) => Err(e),
        };
        out.push(result.unwrap_or_else(|e| AuthStatus {
            retailer: id,
            signed_in: false,
            account: None,
            expires_in: None,
            detail: Some(e.to_string()),
        }));
    }
    out
}

async fn import(app: &App, file: &std::path::Path) -> AppResult<()> {
    let text = std::fs::read_to_string(file).map_err(|e| {
        AppError::usage(format!(
            "cannot read {}: {e}. Export a Netscape cookies.txt from a browser signed in \
             to the shop.",
            file.display()
        ))
    })?;
    let status = app.handle()?.import_cookies(&text).await?;
    show(app, vec![status])
}

async fn status(app: &App) -> AppResult<()> {
    let shops = if app.selected.is_empty() {
        gsnz_core::RetailerId::ALL.to_vec()
    } else {
        app.selected.clone()
    };
    let statuses = statuses_for(app, &shops).await;
    show(app, statuses)
}

/// Signing out drops the credential, so it drops every shop that credential
/// covered. Saying which ones beats letting it be noticed later.
async fn logout(app: &App) -> AppResult<()> {
    let mut statuses = Vec::new();
    let mut out = app.out();
    for target in app.auth_targets() {
        let dropped = app.registry.get(target.through)?.logout().await?;
        let covers = app::name_shops(&target.covers);
        if !out.is_json() {
            if dropped {
                writeln!(out, "Signed out of {covers}.")?;
            } else {
                writeln!(out, "Was not signed in to {covers}.")?;
            }
        }
        statuses.extend(statuses_for(app, &target.covers).await);
    }
    if out.is_json() {
        emit(&mut out, &Statuses(statuses))?;
    }
    Ok(())
}

fn show(app: &App, statuses: Vec<AuthStatus>) -> AppResult<()> {
    emit(&mut app.out(), &Statuses(statuses))?;
    Ok(())
}

/// `--json` gets the array, so `gsnz auth status --json | jq '.[0].signed_in'`
/// works without knowing a wrapper key.
#[derive(Serialize)]
struct Statuses(Vec<AuthStatus>);

impl View for Statuses {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        for s in &self.0 {
            let mark = if s.signed_in {
                out.good("signed in")
            } else {
                out.dim("signed out")
            };
            let who = s.account.as_deref().unwrap_or("");
            writeln!(out, "{}  {mark} {who}", s.retailer)?;
            if let Some(expires_in) = s.expires_in {
                let d = std::time::Duration::from_secs(expires_in);
                writeln!(out, "  expires in {}", cli_kit::human_duration(d))?;
            }
            if let Some(detail) = &s.detail {
                writeln!(out, "  {}", out.dim(detail))?;
            }
        }
        Ok(())
    }
}
