//! `completions` -- the shell script, and nothing else on the stream, so
//! `source <(gsnz completions zsh)` works.

use crate::app::App;
use crate::error::{AppError, AppResult};

pub fn run(app: &App, shell: Option<String>) -> AppResult<()> {
    let named = shell
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<clap_complete::Shell>()
                .map_err(|_| AppError::usage(format!("{s:?} is not a shell this can generate for")))
        })
        .transpose()?;
    let mut out = std::io::stdout();
    cli_kit::completions::generate(
        &mut crate::cli::command(),
        "gsnz",
        named,
        app.env.shell.as_deref(),
        &mut out,
    )
    .map_err(AppError::usage)
}
