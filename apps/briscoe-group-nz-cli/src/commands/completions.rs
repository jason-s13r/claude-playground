//! `completions` -- the shell script, and nothing else on the stream, so
//! `source <(bgnz completions zsh)` works.

use crate::app::App;
use crate::error::{AppError, AppResult};

pub fn run(app: &App, shell: Option<clap_complete::Shell>) -> AppResult<()> {
    let mut out = std::io::stdout();
    cli_kit::completions::generate(
        &mut crate::cli::command(),
        "bgnz",
        shell,
        app.env.shell.as_deref(),
        &mut out,
    )
    .map_err(AppError::usage)
}
