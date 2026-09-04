//! `config` -- reading and writing the settings file.

use cli_kit::{emit, table, Out, View};
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::cli::ConfigAction;
use crate::config::{describe, KEYS};
use crate::error::AppResult;

pub fn run(app: &App, action: ConfigAction) -> AppResult<()> {
    let mut config = app.config.clone();
    match action {
        ConfigAction::List => {
            let settings = KEYS
                .iter()
                .map(|key| {
                    Ok(Setting {
                        key: key.to_string(),
                        value: config.get(key)?,
                        description: describe(key).to_string(),
                    })
                })
                .collect::<AppResult<Vec<_>>>()?;
            emit(&mut app.out(), &Settings { settings })?;
        }
        ConfigAction::Get { key } => {
            // Nothing but the value, so `$(twlnz config get island)` is usable.
            let value = config.get(&key)?;
            let mut out = app.out();
            match value {
                Some(value) => writeln!(out, "{value}")?,
                None => writeln!(out)?,
            }
        }
        ConfigAction::Set { key, value } => {
            config.set(&key, &value)?;
            app.save(&config)?;
            let mut out = app.out();
            writeln!(out, "{key} = {}", config.get(&key)?.unwrap_or_default())?;
        }
        ConfigAction::Unset { key } => {
            config.unset(&key)?;
            app.save(&config)?;
            let mut out = app.out();
            writeln!(out, "{key} is back to its default.")?;
        }
        ConfigAction::Path => {
            let mut out = app.out();
            writeln!(out, "{}", app.config_file.display())?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct Settings {
    settings: Vec<Setting>,
}

#[derive(Serialize)]
struct Setting {
    key: String,
    value: Option<String>,
    description: String,
}

impl View for Settings {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        let mut t = table(&["Setting", "Value", "What it does"]);
        for setting in &self.settings {
            t.add_row(vec![
                setting.key.clone(),
                match &setting.value {
                    Some(value) => value.clone(),
                    None => out.dim("not set"),
                },
                setting.description.clone(),
            ]);
        }
        writeln!(out, "{t}")
    }
}
