//! `auth` -- signing in, and signing out.

use cli_kit::{emit, human_duration, prompt, prompt_password, Out, View};
use serde::Serialize;
use std::io::Write;
use std::time::Duration;

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
        AuthAction::Status => status(app),
        AuthAction::Logout => logout(app),
    }
}

async fn login(
    app: &App,
    email: Option<String>,
    password_command: Option<String>,
    no_store_password: bool,
) -> AppResult<()> {
    let secrets = app.secrets();
    let email = match email {
        Some(email) => email,
        None => prompt("Email")?,
    };

    // A command beats a prompt, so a password manager never has to be typed out
    // of.
    let command = password_command.or_else(|| app.config.auth.password_command.clone());
    let password = match &command {
        Some(command) => {
            net_kit::password::Source::Command(command.clone())
                .password()
                .await?
        }
        None => prompt_password("Password")?,
    };

    // Signed in with a clean session on purpose: reusing whatever is stored
    // would send an expired account cookie along with the form and leave the
    // failure looking like a bad password.
    let http = net_kit::http::build(twlnz_api::client_spec())
        .map_err(|e| AppError::usage(format!("building the HTTP client: {e}")))?;
    let trace: twlnz_api::auth::Trace<'_> = &|m: &str| {
        if app.env.debug {
            eprintln!("twlnz: {m}");
        }
    };
    let session = twlnz_api::auth::login(&http, &app.endpoints(), &email, &password, trace).await?;

    twlnz_api::StoredSession::of(&session, Some(email.clone())).save(&secrets)?;

    // Kept only when it can be used: the renewal path reads the password back,
    // and without one a lapsed session stops every account command until
    // someone signs in by hand.
    let keep = !no_store_password && app.config.auth.store_password && command.is_none();
    if keep {
        net_kit::password::save(&secrets, &password)?;
    }

    emit(
        &mut app.out(),
        &Status {
            signed_in: true,
            account: Some(email),
            expires_in: session
                .expires_at()
                .map(|exp| exp.saturating_sub(net_kit::jwt::now_secs())),
            password_stored: keep,
        },
    )?;
    Ok(())
}

fn status(app: &App) -> AppResult<()> {
    let secrets = app.secrets();
    let stored = twlnz_api::StoredSession::load(&secrets)?;
    let session = stored.as_ref().map(twlnz_api::StoredSession::session);

    emit(
        &mut app.out(),
        &Status {
            signed_in: session.as_ref().is_some_and(twlnz_api::Session::account),
            account: stored.as_ref().and_then(|s| s.email.clone()),
            expires_in: session.as_ref().and_then(|s| {
                s.expires_at()
                    .map(|exp| exp.saturating_sub(net_kit::jwt::now_secs()))
            }),
            password_stored: net_kit::password::load(&secrets).unwrap_or(None).is_some(),
        },
    )?;
    Ok(())
}

fn logout(app: &App) -> AppResult<()> {
    let secrets = app.secrets();
    // Both, always. Leaving the password behind after a logout is the kind of
    // surprise that only shows up much later.
    let had_session = twlnz_api::StoredSession::clear(&secrets)?;
    let had_password = net_kit::password::clear(&secrets).unwrap_or(false);
    emit(
        &mut app.out(),
        &LoggedOut {
            session: had_session,
            password: had_password,
        },
    )
    .map_err(AppError::from)
}

#[derive(Serialize)]
struct Status {
    signed_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    /// Seconds. The account token is a readable JWT, unlike the Woolworths one,
    /// so this is a fact rather than an estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in: Option<u64>,
    password_stored: bool,
}

impl View for Status {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        if !self.signed_in {
            return writeln!(out, "{}. Run `twlnz auth login`.", out.dim("Signed out"));
        }
        let who = self.account.as_deref().unwrap_or("signed in");
        match self.expires_in {
            Some(secs) => writeln!(
                out,
                "{who}, for another {}.",
                human_duration(Duration::from_secs(secs))
            )?,
            None => writeln!(out, "{who}.")?,
        }
        if !self.password_stored {
            writeln!(
                out,
                "{}",
                out.dim(
                    "The password is not stored, so a lapsed session has to be signed in by hand."
                )
            )?;
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct LoggedOut {
    session: bool,
    password: bool,
}

impl View for LoggedOut {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        match (self.session, self.password) {
            (false, false) => writeln!(out, "Nothing to forget."),
            (_, true) => writeln!(out, "Signed out, and the stored password is gone."),
            (true, false) => writeln!(out, "Signed out."),
        }
    }
}
