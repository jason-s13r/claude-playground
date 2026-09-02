//! `wwnz auth` -- signing in, signing out, and reporting on the session.

use anyhow::{bail, Context, Result};
use std::io::Read;

use crate::app::App;
use crate::auth;
use crate::cli::AuthCommand;
use crate::commands::io::{print_json, prompt};
use crate::domain::order::Filter;
use crate::session::{self, Session, StoredSession};

pub async fn run(app: &App, cmd: &AuthCommand) -> Result<bool> {
    match cmd {
        AuthCommand::Login {
            email,
            password_command,
        } => {
            login(app, email.as_deref(), password_command.as_deref()).await?;
            Ok(true)
        }
        AuthCommand::Import { file } => {
            import(app, file)?;
            Ok(true)
        }
        AuthCommand::Logout => {
            logout(app)?;
            Ok(true)
        }
        AuthCommand::Status => status(app).await,
    }
}

async fn login(app: &App, email: Option<&str>, password_command: Option<&str>) -> Result<()> {
    let email = match email.map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) => e.to_string(),
        None => prompt("Email: ")?,
    };

    let command = password_command.or(app.config.password_command.as_deref());
    let password = match command {
        Some(cmd) => password_from_command(cmd)?,
        None => read_password()?,
    };

    let session = auth::login(&app.endpoints, &email, &password).await?;
    StoredSession {
        email: Some(email.clone()),
        cookies: auth::parse_cookie_header(&session.header().unwrap_or_default()),
        obtained_at: session::now(),
    }
    .save(&app.secrets)?;

    if app.json {
        print_json(&serde_json::json!({ "signed_in": true, "email": email }));
    } else {
        println!(
            "Signed in as {email}; session stored in {}.",
            app.secrets.backend().describe()
        );
    }
    Ok(())
}

/// Take the session from a browser's exported cookies.
///
/// The way in when the login flow cannot be followed. Only the session and
/// guest cookies are kept -- an export holds the whole browser's cookies for
/// the site, and none of the rest is a credential this tool should hold on to.
fn import(app: &App, file: &str) -> Result<()> {
    let text = if file == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading cookies from stdin")?;
        buf
    } else {
        std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?
    };

    let cookies = auth::cookies_from_netscape(&text);
    let session = Session::from_cookies(cookies.clone());
    if !session.account {
        bail!(
            "no Woolworths session found in {file}.\n\
             It should be a Netscape-format cookies.txt holding the \
             __session__0 and __session__1 cookies for www.woolworths.co.nz, \
             exported while signed in."
        );
    }

    StoredSession {
        email: None,
        cookies,
        obtained_at: session::now(),
    }
    .save(&app.secrets)?;

    if app.json {
        print_json(&serde_json::json!({ "signed_in": true, "source": file }));
    } else {
        println!("Session imported from {file}. Check it with `wwnz auth status`.");
    }
    Ok(())
}

fn logout(app: &App) -> Result<()> {
    let had_session = StoredSession::clear(&app.secrets)?;
    // The guest token is not a credential, but it is bound to the same cart, so
    // leaving it behind would keep the signed-out session's store selection.
    let had_guest = session::clear_guest(&app.paths);

    if app.json {
        print_json(&serde_json::json!({
            "signed_out": had_session,
            "guest_token_cleared": had_guest,
        }));
    } else if had_session {
        println!("Signed out.");
    } else {
        println!("There was no stored session.");
    }
    Ok(())
}

/// Report whether there is a session and whether it still works.
///
/// A stored session carries no readable expiry -- it is encrypted, and only the
/// site can read it -- so the only honest way to answer "does this still work?"
/// is to make a call with it.
async fn status(app: &App) -> Result<bool> {
    let stored = StoredSession::load(&app.secrets)?;
    let overridden = std::env::var("WWNZ_SESSION").is_ok_and(|v| !v.trim().is_empty());

    let Some(session) = app.stored_session()? else {
        if app.json {
            print_json(&serde_json::json!({ "signed_in": false }));
        } else {
            println!("Not signed in. Run: wwnz auth login --email you@example.com");
        }
        return Ok(false);
    };

    let email = stored.as_ref().and_then(|s| s.email.clone());
    let age = stored
        .as_ref()
        .map(|s| session::now().saturating_sub(s.obtained_at));

    // The cheapest account-scoped call there is: one order, which most
    // accounts will not even have.
    let client = crate::api::Client::new(app.http.clone(), app.endpoints.clone(), session);
    let working = client.orders(1, Filter::All).await;

    if app.json {
        print_json(&serde_json::json!({
            "signed_in": true,
            "email": email,
            "source": if overridden { "WWNZ_SESSION" } else { "stored" },
            "age_seconds": age,
            "working": working.is_ok(),
            "error": working.as_ref().err().map(|e| format!("{e:#}")),
        }));
        return Ok(working.is_ok());
    }

    match email {
        Some(email) => println!("Signed in as {email}"),
        None => println!("Signed in"),
    }
    if overridden {
        println!("Session from WWNZ_SESSION, overriding anything stored.");
    }
    if let Some(age) = age.filter(|_| !overridden) {
        println!(
            "Obtained {} ago",
            crate::commands::io::human_duration(std::time::Duration::from_secs(age),)
        );
    }
    match &working {
        Ok(page) => println!("The session works ({} order(s) on file).", page.total),
        Err(e) => println!("The session no longer works: {e:#}\nSign in again: wwnz auth login"),
    }
    Ok(working.is_ok())
}

/// Run a command and take its first line of stdout as the password.
///
/// The point is a password manager: `password_command = "pass show woolworths"`
/// keeps the password out of the config file and out of the shell history.
fn password_from_command(command: &str) -> Result<String> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .with_context(|| format!("running the password command: {command}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "the password command failed ({}): {}",
            output.status,
            stderr.trim()
        );
    }
    let password = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if password.is_empty() {
        bail!("the password command printed nothing: {command}");
    }
    Ok(password)
}

fn read_password() -> Result<String> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        bail!(
            "a password is required, but there is no terminal to prompt on. \
             Set password_command in the config file, or pass --password-command."
        );
    }
    let password = rpassword::prompt_password("Password: ").context("reading the password")?;
    if password.trim().is_empty() {
        bail!("no password entered");
    }
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_command_gives_up_its_first_line() {
        assert_eq!(
            password_from_command("printf 'hunter2\\nnoise\\n'").unwrap(),
            "hunter2"
        );
    }

    #[test]
    fn a_password_command_that_fails_is_reported_as_such() {
        let err = password_from_command("exit 3").unwrap_err();
        assert!(format!("{err:#}").contains("password command failed"));
    }

    #[test]
    fn a_password_command_that_prints_nothing_is_an_error() {
        let err = password_from_command("true").unwrap_err();
        assert!(format!("{err:#}").contains("printed nothing"));
    }
}
