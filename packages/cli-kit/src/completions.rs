//! Emitting a shell completion script.

use std::io::Write;

use clap_complete::Shell;

/// Write the completion script for `shell` to `out`.
///
/// Nothing but the script goes to the stream, so `source <(tool completions
/// zsh)` works. The caller runs this before any config or credential handling:
/// generating a script must not depend on the machine being set up.
///
/// `shell_env` is the caller's reading of `$SHELL`, since this crate does not
/// read the environment. `None` for both arguments is an error naming the
/// shells on offer, rather than a guess.
pub fn generate(
    cmd: &mut clap::Command,
    bin: &str,
    shell: Option<Shell>,
    shell_env: Option<&str>,
    out: &mut dyn Write,
) -> Result<(), String> {
    let shell = shell
        .or_else(|| shell_env.and_then(from_path))
        .ok_or_else(|| {
            format!(
                "could not tell which shell this is; name one:\n  \
                 {bin} completions <bash|zsh|fish|powershell|elvish>"
            )
        })?;
    clap_complete::generate(shell, cmd, bin, out);
    Ok(())
}

/// The shell named by a `$SHELL` path such as `/bin/zsh`.
pub fn from_path(shell_env: &str) -> Option<Shell> {
    let name = shell_env.rsplit('/').next()?;
    match name {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        "elvish" => Some(Shell::Elvish),
        "pwsh" | "powershell" => Some(Shell::PowerShell),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command() -> clap::Command {
        clap::Command::new("demo").subcommand(clap::Command::new("search"))
    }

    #[test]
    fn reads_the_shell_out_of_a_shell_path() {
        assert_eq!(from_path("/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(from_path("/usr/local/bin/fish"), Some(Shell::Fish));
        assert_eq!(from_path("bash"), Some(Shell::Bash));
        assert_eq!(from_path("/bin/some-other-shell"), None);
    }

    #[test]
    fn an_explicit_shell_beats_the_environment() {
        let mut buf = Vec::new();
        generate(
            &mut command(),
            "demo",
            Some(Shell::Bash),
            Some("/bin/zsh"),
            &mut buf,
        )
        .unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("complete"), "bash script: {text:.60}");
    }

    #[test]
    fn falls_back_to_the_shell_path() {
        let mut buf = Vec::new();
        generate(&mut command(), "demo", None, Some("/bin/zsh"), &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("#compdef demo"), "zsh script: {text:.60}");
    }

    #[test]
    fn with_nothing_to_go_on_it_names_the_options_rather_than_guessing() {
        let mut buf = Vec::new();
        let err = generate(&mut command(), "demo", None, None, &mut buf).unwrap_err();
        assert!(err.contains("demo completions <bash|zsh"), "{err}");
        assert!(buf.is_empty(), "nothing written on failure");
    }
}
