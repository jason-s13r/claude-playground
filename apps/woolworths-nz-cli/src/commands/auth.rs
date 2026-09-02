//! `wwnz auth` -- signing in, signing out, and reporting on the session.

use anyhow::{bail, Context, Result};
use std::io::Read;

use crate::app::App;
use crate::auth;
use crate::cli::AuthCommand;
use crate::commands::io::{print_json, prompt};
use crate::domain::order::Filter;
use crate::password;
use crate::session::{self, Session, StoredSession};

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

async fn login(
    app: &App,
    email: Option<&str>,
    password_command: Option<&str>,
    store_password: bool,
) -> Result<()> {
    let email = match email.map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) => e.to_string(),
        None => prompt("Email: ")?,
    };

    let command = password_command.or(app.config.password_command.as_deref());
    let password = match command {
        Some(cmd) => password::from_command(cmd)?,
        None => read_password()?,
    };
    // Not conditional on where the password came from: `--password-command` is
    // the only way to sign in without a terminal, so refusing to store what it
    // printed would mean a headless box could never set this up.
    let store_password = store_password && app.config.store_password.unwrap_or(true);

    let session = auth::login(&app.endpoints, &email, &password).await?;
    StoredSession {
        email: Some(email.clone()),
        cookies: session.cookies(),
        obtained_at: session::now(),
    }
    .save(&app.secrets)?;
    // The session cookie is encrypted and has nothing to refresh it with, so
    // the password is the only thing that can renew it unattended.
    if store_password {
        password::save(&app.secrets, &password)?;
    } else {
        password::clear(&app.secrets)?;
    }

    if app.json {
        print_json(&serde_json::json!({
            "signed_in": true,
            "email": email,
            "password_stored": store_password,
        }));
    } else {
        println!(
            "Signed in as {email}; session stored in {}.",
            app.secrets.backend().describe()
        );
        if store_password {
            println!(
                "Password kept there as well, so a lapsed session signs itself in\n\
                 again. `--no-store-password` skips that."
            );
        } else if command.is_some() {
            println!("Password not stored; password_command signs in again instead.");
        } else {
            println!(
                "Password not stored, so a lapsed session needs `wwnz auth login`\n\
                 again. Set password_command, or drop --no-store-password."
            );
        }
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
        // An export says nothing about who it belongs to, and without an email
        // there is nobody to sign back in as -- so an imported session cannot
        // renew itself even where a password is stored.
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
    let had_password = password::clear(&app.secrets)?;
    // The guest token is not a credential, but it is bound to the same cart, so
    // leaving it behind would keep the signed-out session's store selection.
    let had_guest = session::clear_guest(&app.paths);

    if app.json {
        print_json(&serde_json::json!({
            "signed_out": had_session,
            "had_password": had_password,
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

    // Whether a lapsed session could come back on its own. It takes an email
    // as well as a password, so a session from `auth import` or `WWNZ_SESSION`
    // -- neither of which names one -- has no renewal even where a password is
    // stored.
    let renewal = match email.is_some() && !overridden {
        true => password::Source::resolve(app.config.password_command.as_deref(), &app.secrets)?,
        false => None,
    };

    // The cheapest account-scoped call there is: one order, which most
    // accounts will not even have. Deliberately built without renewal, so this
    // reports the session as it stands rather than quietly replacing it.
    let client = crate::api::Client::new(app.http.clone(), app.endpoints.clone(), session);
    let working = client.orders(1, Filter::All).await;
    let usable = working.is_ok() || renewal.is_some();

    if app.json {
        print_json(&serde_json::json!({
            "signed_in": true,
            "email": email,
            "source": if overridden { "WWNZ_SESSION" } else { "stored" },
            "age_seconds": age,
            "working": working.is_ok(),
            "usable": usable,
            "unattended_renewal": renewal.as_ref().map(|r| r.describe()),
            "error": working.as_ref().err().map(|e| format!("{e:#}")),
        }));
        return Ok(usable);
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
    match (&working, &renewal) {
        (Ok(page), _) => println!("The session works ({} order(s) on file).", page.total),
        (Err(e), Some(r)) => println!(
            "The session no longer works: {e:#}\nThe next command signs in again from {}.",
            r.describe()
        ),
        (Err(e), None) => {
            println!("The session no longer works: {e:#}\nSign in again: wwnz auth login")
        }
    }
    if working.is_ok() {
        if let Some(r) = &renewal {
            println!("Renewal: automatic, from {}.", r.describe());
        }
    }
    Ok(usable)
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
