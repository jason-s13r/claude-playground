//! `config` -- reading and writing the settings file.

use cli_kit::{emit, table, Out, View};
use serde::Serialize;
use std::io::Write;

use crate::app::App;
use crate::cli::ConfigAction;
use crate::config::SETTINGS;
use crate::error::AppResult;

pub fn run(app: &App, action: ConfigAction) -> AppResult<()> {
    let mut config = app.config.clone();
    match action {
        ConfigAction::List => {
            let settings = SETTINGS
                .iter()
                .map(|setting| {
                    Ok(Row {
                        key: setting.key.to_string(),
                        value: config.get(setting.key)?,
                        description: setting.what.to_string(),
                    })
                })
                .collect::<AppResult<Vec<_>>>()?;
            emit(&mut app.out(), &Settings { settings })?;
        }
        ConfigAction::Get { key } => {
            // Nothing but the value, so `$(kmart config get country)` is
            // usable.
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
    settings: Vec<Row>,
}

#[derive(Serialize)]
struct Row {
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
