//! `wwnz completions` -- emit a shell completion script.

use anyhow::{bail, Result};
use clap::CommandFactory;
use clap_complete::Shell;
use std::io::stdout;

use crate::cli::Cli;

/// Write the completion script for `shell` to stdout.
///
/// Nothing but the script goes to stdout, so `source <(wwnz completions zsh)`
/// works. Runs before any config or credential handling: generating a script
/// must not depend on the machine being set up.
pub fn run(shell: Option<Shell>) -> Result<()> {
    let shell = match shell.or_else(Shell::from_env) {
        Some(s) => s,
        None => bail!(
            "could not tell which shell this is from $SHELL; name one:\n  \
             wwnz completions <bash|zsh|fish|powershell|elvish>"
        ),
    };
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "wwnz", &mut stdout());
    Ok(())
}
