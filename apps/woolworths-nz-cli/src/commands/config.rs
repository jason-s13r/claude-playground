//! `config` -- reading and changing the settings file.
//!
//! Every write goes through the typed [`crate::config::Config`], so a value
//! that will not parse is refused here rather than at the next command that
//! reads it. Editing the file by hand is still allowed; this exists so it is
//! not *required*.

use cli_kit::{emit, Out, View};
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::cli::ConfigAction;
use crate::config::{describe, KEYS};
use crate::error::AppResult;

pub fn run(app: &App, action: ConfigAction) -> AppResult<()> {
    match action {
        ConfigAction::List => list(app),
        ConfigAction::Get { key } => get(app, &key),
        ConfigAction::Set { key, value } => write(app, &key, Some(&value)),
        ConfigAction::Unset { key } => write(app, &key, None),
        ConfigAction::Path => {
            writeln!(app.out(), "{}", app.config_file.display())?;
            Ok(())
        }
    }
}

fn list(app: &App) -> AppResult<()> {
    let settings = KEYS
        .iter()
        .map(|key| Setting {
            key,
            value: app.config.get(key).unwrap_or(None),
            about: describe(key),
        })
        .collect();
    emit(&mut app.out(), &Settings(settings))?;
    Ok(())
}

/// One value on its own line, so `$(wwnz config get store_id)` is usable.
fn get(app: &App, key: &str) -> AppResult<()> {
    let mut out = app.out();
    match app.config.get(key)? {
        Some(value) if out.is_json() => emit(&mut out, &Value(Some(value)))?,
        Some(value) => writeln!(out, "{value}")?,
        None if out.is_json() => emit(&mut out, &Value(None))?,
        // Nothing on stdout: a script reading this should see an empty
        // string, not the word "none".
        None => eprintln!("wwnz: {key} is not set"),
    }
    Ok(())
}

fn write(app: &App, key: &str, value: Option<&str>) -> AppResult<()> {
    let mut config = app.config.clone();
    match value {
        Some(value) => config.set(key, value)?,
        None => config.unset(key)?,
    }
    net_kit::config::save_toml(&app.config_file, &config)?;

    // Read back what was stored rather than echoing what was typed, so the
    // file's contents stay predictable.
    let stored = config.get(key)?;
    let mut out = app.out();
    if out.is_json() {
        emit(&mut out, &Value(stored))?;
    } else {
        match stored {
            Some(stored) => writeln!(out, "{key} = {stored}")?,
            None => writeln!(out, "{key} is unset")?,
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct Setting {
    key: &'static str,
    value: Option<String>,
    about: &'static str,
}

#[derive(Serialize)]
struct Settings(Vec<Setting>);

#[derive(Serialize)]
struct Value(Option<String>);

impl View for Value {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        match &self.0 {
            Some(value) => writeln!(out, "{value}"),
            None => Ok(()),
        }
    }

    fn json(&self) -> cli_kit::serde_json::Value {
        cli_kit::serde_json::to_value(&self.0).unwrap_or(cli_kit::serde_json::Value::Null)
    }
}

impl View for Settings {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        let width = self.0.iter().map(|s| s.key.len()).max().unwrap_or(0);
        for setting in &self.0 {
            match &setting.value {
                Some(value) => writeln!(out, "{:<width$}  {value}", setting.key)?,
                // Not blank: "unset" and "set to an empty string" are
                // different, and only one of them is normal.
                None => writeln!(out, "{:<width$}  {}", setting.key, out.dim("unset"))?,
            }
            writeln!(out, "{:<width$}  {}", "", out.dim(setting.about))?;
        }
        Ok(())
    }

    fn json(&self) -> cli_kit::serde_json::Value {
        cli_kit::serde_json::to_value(&self.0).unwrap_or(cli_kit::serde_json::Value::Null)
    }
}

/// Every key is reachable, so `config list` cannot drift from what `get`
/// understands.
#[cfg(test)]
mod tests {
    use crate::config::{Config, KEYS};

    #[test]
    fn every_listed_key_can_be_read() {
        let config = Config::default();
        for key in KEYS {
            config
                .get(key)
                .unwrap_or_else(|e| panic!("{key} is listed but not readable: {e}"));
        }
    }

    #[test]
    fn an_unknown_key_says_how_to_find_the_real_ones() {
        let err = Config::default().get("store").unwrap_err();
        assert!(err.to_string().contains("wwnz config list"), "{err}");
    }
}
