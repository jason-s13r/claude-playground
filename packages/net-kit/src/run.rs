//! Running a command the user configured, and taking its stdout.
//!
//! This is how a password or a token gets in without being written down: the
//! config names a command, the command prints the secret.

use crate::error::{Error, Result};

/// Run `cmd` through a shell-style split and return its trimmed stdout.
///
/// `what` names the setting in any failure, so "password_command failed" points
/// at the thing to fix rather than at this function.
pub async fn capturing(what: &str, cmd: &str) -> Result<String> {
    let parts = shell_words::split(cmd).map_err(|e| Error::Command {
        command: what.to_string(),
        detail: format!("could not parse {cmd:?}: {e}"),
    })?;
    let (program, args) = parts.split_first().ok_or_else(|| Error::Command {
        command: what.to_string(),
        detail: "is empty".into(),
    })?;

    let out = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(|e| Error::Command {
            command: what.to_string(),
            detail: format!("could not run {cmd:?}: {e}"),
        })?;

    if !out.status.success() {
        return Err(Error::Command {
            command: what.to_string(),
            detail: format!(
                "exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }

    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if value.is_empty() {
        return Err(Error::Command {
            command: what.to_string(),
            detail: "printed nothing on stdout".into(),
        });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn takes_trimmed_stdout() {
        let out = capturing("password_command", "printf 'hunter2\\n'").await;
        // `printf` with no shell: the split gives argv, so the escape is the
        // program's, not a shell's.
        assert_eq!(out.unwrap(), "hunter2");
    }

    #[tokio::test]
    async fn a_failing_command_names_the_setting() {
        let err = capturing("password_command", "false").await.unwrap_err();
        let text = err.to_string();
        assert!(text.contains("password_command"), "{text}");
    }

    #[tokio::test]
    async fn silence_is_a_failure_not_an_empty_secret() {
        let err = capturing("token_command", "true").await.unwrap_err();
        assert!(err.to_string().contains("printed nothing"), "{err}");
    }

    #[tokio::test]
    async fn an_empty_command_is_rejected_before_spawning() {
        let err = capturing("token_command", "   ").await.unwrap_err();
        assert!(err.to_string().contains("is empty"), "{err}");
    }

    #[tokio::test]
    async fn a_missing_program_is_reported_not_panicked() {
        let err = capturing("token_command", "definitely-not-a-real-program-xyz")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("could not run"), "{err}");
    }
}
