//! `auth` -- getting the two credentials, and giving them up.
//!
//! Two independent things, and this command handles both because they fail in
//! ways a person would confuse: a **bearer token** says who you are, and
//! **Akamai cookies** say you are a browser. Having one without the other is
//! normal, and `status` reports them apart.
//!
//! Both come from a browser, for different reasons. The cookies because
//! passing the bot check means running its script. The token because the one
//! step of Auth0's login that would mint one -- the password submit -- is
//! behind that same check; see `kmart_api::auth`. The token half is the
//! better bargain, though: it is a *refresh* token, and the endpoint that
//! spends it is not guarded, so it is copied once and renews itself
//! thereafter. The cookies last about a day.

use std::io::{Read, Write};
use std::time::Duration;

use cli_kit::{emit, human_duration, prompt, prompt_password, Out, View};
use kmart_api::{Country, Session, StoredSession};
use serde::Serialize;

use crate::app::App;
use crate::cli::AuthAction;
use crate::error::{AppError, AppResult};

pub async fn run(app: &App, action: AuthAction) -> AppResult<()> {
    match action {
        AuthAction::Login {
            email,
            password_command,
            no_store_password,
            headless,
        } => login(app, email, password_command, no_store_password, headless).await,
        AuthAction::Import { file } => import(app, &file),
        AuthAction::Token { token } => token_import(app, token).await,
        AuthAction::Status => status(app),
        AuthAction::Logout => logout(app),
    }
}

async fn login(
    app: &App,
    email: Option<String>,
    password_command: Option<String>,
    no_store_password: bool,
    headless: bool,
) -> AppResult<()> {
    let secrets = app.secrets();
    let email = match email {
        Some(email) => email,
        None => prompt("Email")?,
    };

    // A command beats a prompt, so a password manager never has to be typed
    // out of -- `--password-command 'op read "op://Personal/Kmart/password"'`
    // and the password never touches this process's output or its arguments.
    let command = password_command.or_else(|| app.config.auth.password_command.clone());
    let password = match &command {
        Some(command) => {
            net_kit::password::Source::Command(command.clone())
                .password()
                .await?
        }
        None => prompt_password("Password")?,
    };

    let mut out = app.out();
    if !out.is_json() {
        writeln!(
            out,
            "{}",
            out.dim("Opening a browser. Kmart's bot check refuses anything else.")
        )?;
    }

    let signed_in = crate::browser::login(
        app.env.browser_python.as_deref(),
        &app.paths.state_dir,
        app.country.origin(),
        &email,
        &password,
        headless,
        app.env.debug,
    )
    .await?;

    // One run earns both credentials, so both are kept. The cookies are
    // merged rather than replacing, so a country this run did not visit keeps
    // whatever it had.
    let mut stored = StoredSession::load(&secrets)?.unwrap_or_default();
    stored.tokens = Some(kmart_api::Tokens::from_refresh(&signed_in.refresh_token));
    stored.email = signed_in.email.clone().or(Some(email.clone()));
    // Which storefront minted it. The two countries are separate Auth0
    // applications, so this is what a later renewal needs to pick the right
    // one -- a session signed in on kmart.com.au cannot be renewed as the New
    // Zealand app, whatever country the next command asks about.
    stored.auth_country = Some(app.country.code().to_string());
    for (code, cookies) in &signed_in.cookies {
        if !cookies.is_empty() {
            stored.cookies.insert(code.clone(), cookies.clone());
        }
    }
    stored.save(&secrets)?;

    // Kept only when it can be used. Less load-bearing here than for the other
    // tools in this repo -- the refresh token renews without it -- so this is
    // for the day that token is refused.
    let keep = !no_store_password && app.config.auth.store_password && command.is_none();
    if keep {
        net_kit::password::save(&secrets, &password)?;
    }

    let session = stored.session();
    emit(
        &mut app.out(),
        &status_of(app, &session, stored.email.clone(), keep),
    )?;
    Ok(())
}

fn import(app: &App, file: &str) -> AppResult<()> {
    let text = match file {
        "-" => {
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            buffer
        }
        path => std::fs::read_to_string(path)?,
    };

    let imported = kmart_api::auth::session_from_netscape(&text);
    let admitted: Vec<Country> = Country::ALL
        .into_iter()
        .filter(|c| imported.admitted(*c))
        .collect();
    if admitted.is_empty() {
        return Err(kmart_api::Error::NoSession {
            detail: format!(
                ": {} carried no Kmart bot-check cookies. Sign in at kmart.co.nz or \
                 kmart.com.au first, then export while that tab is open",
                match file {
                    "-" => "standard input",
                    path => path,
                }
            ),
        }
        .into());
    }

    // Merged rather than replacing: an import for one country must not throw
    // away the other country's cookies or the signed-in token.
    let secrets = app.secrets();
    let mut stored = StoredSession::load(&secrets)?.unwrap_or_default();
    for country in &admitted {
        stored.cookies.insert(
            country.code().to_string(),
            kmart_api::auth::from_netscape(&text, *country),
        );
    }
    stored.save(&secrets)?;

    let mut out = app.out();
    emit(
        &mut out,
        &Imported {
            countries: admitted.iter().map(|c| c.to_string()).collect(),
        },
    )?;
    Ok(())
}

/// Keep a refresh token lifted out of a browser.
///
/// **Spent once, here, to check it.** The alternative -- storing whatever was
/// typed and letting the next command find out -- accepts a placeholder or a
/// half-copied token without complaint and then fails somewhere that reads as
/// a Kmart problem rather than a paste problem.
///
/// Auth0 refusing the token and the endpoint being unreachable are different
/// answers and are treated differently: the first refuses the import, the
/// second keeps it and says it could not be checked. Conflating them would
/// make a flaky connection look like a bad credential.
async fn token_import(app: &App, token: Option<String>) -> AppResult<()> {
    let token = match token {
        Some(token) => token,
        // Hidden, and off the shell history.
        None => prompt_password("Refresh token")?,
    };
    let token = token.trim();
    if token.is_empty() {
        return Err(AppError::usage("no token given"));
    }
    // The commonest paste mistake is the whole local-storage entry rather than
    // the field inside it, and the resulting failure is otherwise a baffling
    // `invalid_grant` from Auth0 much later.
    if token.starts_with('{') {
        return Err(AppError::usage(concat!(
            "that looks like the whole local-storage entry; ",
            "copy the value of its `refresh_token` field only"
        )));
    }

    let trace: kmart_api::auth::Trace<'_> = &|step: &str, detail: &str| {
        if app.env.debug {
            eprintln!("kmart: [{step}] {detail}");
        }
    };
    let checked = kmart_api::auth::refresh(&app.endpoints(), token, trace).await;

    let (tokens, verified) = match checked {
        // Auth0 kept its word. Store what it gave back, including the access
        // token, so the next command does not have to spend a second call.
        Ok(mut fresh) => {
            // Auth0 rotates the refresh token only sometimes; keeping the
            // original when it does not is the difference between a session
            // that renews forever and one that dies in fifteen minutes.
            if fresh.refresh.is_none() {
                fresh.refresh = Some(token.to_string());
            }
            (fresh, true)
        }
        // Auth0 answered, and said no. The token is the problem.
        Err(e @ kmart_api::Error::LoginRefused { .. }) => return Err(e.into()),
        Err(kmart_api::Error::Challenged { host }) => {
            return Err(kmart_api::Error::Challenged { host }.into())
        }
        // Nobody answered. Not evidence about the token either way, so it is
        // kept and the next command will find out.
        Err(_) => (kmart_api::Tokens::from_refresh(token), false),
    };

    let secrets = app.secrets();
    let mut stored = kmart_api::StoredSession::load(&secrets)?.unwrap_or_default();
    stored.tokens = Some(tokens);
    // A token pasted in came from a browser on one of the two storefronts,
    // and `--country` is the only statement of which.
    stored.auth_country = Some(app.country.code().to_string());
    stored.save(&secrets)?;

    let mut out = app.out();
    match verified {
        true => {
            writeln!(out, "Refresh token accepted by Kmart and kept.")?;
            writeln!(
                out,
                "{}",
                out.dim("It renews itself, so unlike the cookies this is a one-off.")
            )?;
        }
        false => {
            writeln!(
                out,
                "Refresh token kept, but Kmart could not be reached to check it."
            )?;
            writeln!(
                out,
                "{}",
                out.dim("Run `kmart auth status` once you are online.")
            )?;
        }
    }
    Ok(())
}

fn status(app: &App) -> AppResult<()> {
    let secrets = app.secrets();
    let stored = StoredSession::load(&secrets)?;
    let session = stored
        .as_ref()
        .map(StoredSession::session)
        .unwrap_or_default();
    let email = stored.as_ref().and_then(|s| s.email.clone());
    let password_stored = net_kit::password::load(&secrets).unwrap_or(None).is_some();

    emit(
        &mut app.out(),
        &status_of(app, &session, email, password_stored),
    )?;
    Ok(())
}

fn status_of(
    app: &App,
    session: &Session,
    account: Option<String>,
    password_stored: bool,
) -> Status {
    Status {
        signed_in: session.signed_in(),
        account,
        expires_in: session
            .tokens()
            .map(|t| t.expires_at.saturating_sub(net_kit::jwt::now_secs())),
        renewable: session.tokens().is_some_and(|t| t.refresh.is_some()),
        admitted: Country::ALL
            .into_iter()
            .filter(|c| session.admitted(*c))
            .map(|c| c.to_string())
            .collect(),
        country: app.country.to_string(),
        password_stored,
    }
}

fn logout(app: &App) -> AppResult<()> {
    let secrets = app.secrets();
    // All of it, always. Leaving the password behind after a logout is the
    // kind of surprise that only shows up much later.
    let had_session = StoredSession::clear(&secrets)?;
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
    /// Seconds. The access token is a readable JWT, so this is a fact rather
    /// than an estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in: Option<u64>,
    /// Whether it renews without a password.
    renewable: bool,
    /// The countries whose gateway will answer, which is a separate question
    /// from being signed in.
    admitted: Vec<String>,
    country: String,
    password_stored: bool,
}

impl View for Status {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        match self.signed_in {
            // Not `auth login`: Kmart's bot check blocks the password submit,
            // so pointing there would send someone somewhere that cannot work.
            false => writeln!(
                out,
                "{}. Run `kmart auth token` with a refresh token from a browser.",
                out.dim("Signed out")
            )?,
            true => {
                let who = self.account.as_deref().unwrap_or("Signed in");
                match self.expires_in {
                    // A token that has run out is not a problem when it
                    // renews, so the two are said in one line rather than
                    // reported as a failure.
                    Some(0) if self.renewable => {
                        writeln!(out, "{who}. The token has lapsed and will renew.")?
                    }
                    Some(secs) => writeln!(
                        out,
                        "{who}, for another {}.",
                        human_duration(Duration::from_secs(secs))
                    )?,
                    None => writeln!(out, "{who}.")?,
                }
                if !self.renewable && !self.password_stored {
                    writeln!(
                        out,
                        "{}",
                        out.dim(
                            "No refresh token and no stored password, so this will \
                             need signing in again."
                        )
                    )?;
                }
            }
        }

        // The half people forget, and the one that expires first.
        match self.admitted.is_empty() {
            true => writeln!(
                out,
                "No bot-check cookies. Stock, stores and the account commands will be \
                 refused: run `kmart auth import <cookies.txt>`."
            )?,
            false => {
                writeln!(out, "Bot-check cookies for: {}.", self.admitted.join(", "))?;
                if !self.admitted.contains(&self.country) {
                    writeln!(
                        out,
                        "{}",
                        out.dim(&format!(
                            "None for {}, which is the country in use.",
                            self.country
                        ))
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct Imported {
    countries: Vec<String>,
}

impl View for Imported {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        writeln!(
            out,
            "Bot-check cookies imported for: {}.",
            self.countries.join(", ")
        )?;
        // The single most useful thing to know about them.
        writeln!(out, "{}", out.dim("They last about a day."))
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

#[cfg(test)]
mod tests {
    use super::*;
    use cli_kit::Format;

    fn render(status: &Status) -> String {
        let mut out = Out::buffer(Format::Text);
        emit(&mut out, status).unwrap();
        out.into_string()
    }

    fn status(signed_in: bool, admitted: Vec<&str>) -> Status {
        Status {
            signed_in,
            account: Some("shopper@example.test".into()),
            expires_in: Some(900),
            renewable: true,
            admitted: admitted.into_iter().map(str::to_string).collect(),
            country: "nz".into(),
            password_stored: false,
        }
    }

    #[test]
    fn the_two_credentials_are_reported_separately() {
        // Signed in but not admitted is the normal state after a login, and
        // telling someone to sign in again would be the wrong advice.
        let text = render(&status(true, vec![]));
        assert!(text.contains("shopper@example.test"), "{text}");
        assert!(text.contains("auth import"), "{text}");
        assert!(!text.contains("auth login"), "{text}");

        // Admitted but not signed in is the other half.
        let text = render(&status(false, vec!["nz"]));
        assert!(text.contains("auth token"), "{text}");
        assert!(
            !text.contains("auth login"),
            "the password flow cannot work, so nothing may point at it: {text}"
        );
        assert!(text.contains("Bot-check cookies for: nz"), "{text}");
    }

    #[test]
    fn cookies_for_the_wrong_country_are_called_out() {
        let text = render(&status(true, vec!["au"]));
        assert!(text.contains("None for nz"), "{text}");
    }

    #[test]
    fn a_lapsed_but_renewable_token_is_not_reported_as_a_problem() {
        let mut s = status(true, vec!["nz"]);
        s.expires_in = Some(0);
        let text = render(&s);
        assert!(text.contains("will renew"), "{text}");
    }

    #[test]
    fn no_refresh_token_and_no_password_is_the_case_worth_warning_about() {
        let mut s = status(true, vec!["nz"]);
        s.renewable = false;
        s.password_stored = false;
        assert!(render(&s).contains("need signing in again"));
    }
}
