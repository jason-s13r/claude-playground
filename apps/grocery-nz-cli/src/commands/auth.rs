//! `auth` -- signing in, and the four ways a session ends.

use cli_kit::{emit, prompt, prompt_password, Out, View};
use gsnz_core::AuthStatus;
use serde::Serialize;
use std::io::Write;

use crate::app::App;
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
        AuthAction::Refresh => {
            let status = app.handle()?.refresh_session().await?;
            show(app, vec![status])
        }
        AuthAction::Status => status(app).await,
        AuthAction::Logout => logout(app).await,
    }
}

async fn login(
    app: &App,
    email: Option<String>,
    password_command: Option<String>,
    no_store_password: bool,
) -> AppResult<()> {
    let retailer = app.retailer()?;
    let handle = app.handle()?;

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

    // Never echoed, never logged: the code goes straight back to Club Plus.
    let ask = |method: &str| {
        eprintln!("A verification code was sent by {method}.");
        prompt("Code")
    };
    let status = handle.login(&email, &password, &ask).await?;

    // Only after the login worked: storing a password that does not sign in is
    // worse than storing none.
    let keep = app.config.auth.store_password && !no_store_password;
    if keep {
        let secrets = app.secrets(retailer);
        if let Err(e) = net_kit::password::save(&secrets, &password) {
            eprintln!("gsnz: signed in, but the password could not be kept: {e}");
        }
    }
    show(app, vec![status])
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
    let mut statuses = Vec::new();
    for id in shops {
        // Asking about one shop must not fail because another's stored blob is
        // unreadable: `auth status` is what someone runs to find that out.
        match app.registry.get(id) {
            Ok(handle) => match handle.auth_status().await {
                Ok(status) => statuses.push(status),
                Err(e) => statuses.push(AuthStatus {
                    retailer: id,
                    signed_in: false,
                    account: None,
                    expires_in: None,
                    detail: Some(e.to_string()),
                }),
            },
            Err(e) => statuses.push(AuthStatus {
                retailer: id,
                signed_in: false,
                account: None,
                expires_in: None,
                detail: Some(e.to_string()),
            }),
        }
    }
    show(app, statuses)
}

async fn logout(app: &App) -> AppResult<()> {
    let retailer = app.retailer()?;
    let dropped = app.handle()?.logout().await?;
    let mut out = app.out();
    if out.is_json() {
        emit(
            &mut out,
            &Statuses(vec![AuthStatus {
                retailer,
                signed_in: false,
                account: None,
                expires_in: None,
                detail: None,
            }]),
        )?;
    } else if dropped {
        writeln!(out, "Signed out of {retailer}.")?;
    } else {
        writeln!(out, "Was not signed in to {retailer}.")?;
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
