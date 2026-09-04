//! `auth` -- signing in, and the four ways a session ends.

use cli_kit::{emit, prompt, prompt_password, Out, View};
use gsnz_core::AuthStatus;
use serde::Serialize;
use std::io::Write;

use crate::app::{App, RETAILER};
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
        AuthAction::Status => show(app, status(app).await),
        AuthAction::Logout => logout(app).await,
    }
}

async fn login(
    app: &App,
    email: Option<String>,
    password_command: Option<String>,
    no_store_password: bool,
) -> AppResult<()> {
    let handle = app.handle()?;
    // On stderr with the prompts: what is being asked for is part of the
    // conversation, not part of the result.
    let email = match email {
        Some(email) => email,
        None => prompt("Email")?,
    };
    // A configured command is the account's real source of truth where one
    // exists, so it beats both the prompt and anything stored.
    let password = match password_command
        .as_deref()
        .or(app.config.auth.password_command.as_deref())
    {
        Some(command) => net_kit::run::capturing("password_command", command).await?,
        None => prompt_password("Password")?,
    };

    // Auth0 challenges nothing this flow can answer, so a code prompt here
    // would be a prompt nothing ever reaches. It exists because the trait has
    // one; a challenge fails the login instead.
    let ask = |method: &str| {
        eprintln!("A verification code was sent by {method}.");
        prompt("Code")
    };
    handle.login(&email, &password, &ask).await?;

    // Only after the login worked: storing a password that does not sign in is
    // worse than storing none. And it matters more here than elsewhere --
    // signing in again is the only renewal a Woolworths session has.
    if app.config.auth.store_password && !no_store_password {
        if let Err(e) = net_kit::password::save(&app.secrets(), &password) {
            eprintln!("wwnz: signed in, but the password could not be kept: {e}");
        }
    }
    show(app, status(app).await)
}

/// Sign in again from the stored password, which is the only renewal there is:
/// the session cookie is encrypted and only the site can mint one.
///
/// An account that was never signed in is reported, not failed.
async fn refresh(app: &App) -> AppResult<()> {
    let handle = app.handle()?;
    if let Ok(status) = handle.auth_status().await {
        if !status.signed_in {
            eprintln!("wwnz: not signed in, so there is nothing to renew");
            return show(app, status);
        }
    }
    let result = handle.refresh_session().await;
    // The status is the useful part and is worth printing either way; the
    // failure still decides the exit code.
    let status = status(app).await;
    show(app, status)?;
    result?;
    Ok(())
}

/// What the account reports, with a failure reported rather than raised.
///
/// `auth status` is what someone runs to find out that a stored blob is
/// unreadable, so it must not be the command that fails because of one.
async fn status(app: &App) -> AuthStatus {
    let result = match app.handle() {
        Ok(handle) => handle.auth_status().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    result.unwrap_or_else(|detail| AuthStatus {
        retailer: RETAILER,
        signed_in: false,
        account: None,
        expires_in: None,
        detail: Some(detail),
    })
}

async fn import(app: &App, file: &std::path::Path) -> AppResult<()> {
    let text = std::fs::read_to_string(file).map_err(|e| {
        AppError::usage(format!(
            "cannot read {}: {e}. Export a Netscape cookies.txt from a browser signed in \
             to woolworths.co.nz.",
            file.display()
        ))
    })?;
    let status = app.handle()?.import_cookies(&text).await?;
    show(app, status)
}

async fn logout(app: &App) -> AppResult<()> {
    let dropped = app.handle()?.logout().await?;
    let mut out = app.out();
    if out.is_json() {
        emit(&mut out, &Status(status(app).await))?;
    } else if dropped {
        writeln!(out, "Signed out.")?;
    } else {
        writeln!(out, "Was not signed in.")?;
    }
    Ok(())
}

fn show(app: &App, status: AuthStatus) -> AppResult<()> {
    emit(&mut app.out(), &Status(status))?;
    Ok(())
}

/// One account, so `--json` gets the object rather than a one-element array:
/// `wwnz auth status --json | jq .signed_in` should not have to index.
#[derive(Serialize)]
struct Status(AuthStatus);

impl View for Status {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        let s = &self.0;
        let mark = if s.signed_in {
            out.good("signed in")
        } else {
            out.dim("signed out")
        };
        match s.account.as_deref() {
            Some(who) => writeln!(out, "{}  {mark} {who}", s.retailer)?,
            None => writeln!(out, "{}  {mark}", s.retailer)?,
        }
        if let Some(expires_in) = s.expires_in {
            let d = std::time::Duration::from_secs(expires_in);
            writeln!(out, "  expires in {}", cli_kit::human_duration(d))?;
        }
        if let Some(detail) = &s.detail {
            writeln!(out, "  {}", out.dim(detail))?;
        }
        Ok(())
    }

    fn json(&self) -> cli_kit::serde_json::Value {
        cli_kit::serde_json::to_value(&self.0).unwrap_or(cli_kit::serde_json::Value::Null)
    }
}
