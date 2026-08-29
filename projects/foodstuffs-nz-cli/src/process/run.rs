//! Running a command the user configured, and taking its stdout.

use anyhow::{anyhow, bail, Context, Result};

/// Run `cmd` through a shell-style split and return its trimmed stdout.
pub async fn capturing(cmd: &str) -> Result<String> {
    let parts = shell_words::split(cmd).with_context(|| format!("parsing command: {cmd}"))?;
    let (program, args) = parts
        .split_first()
        .ok_or_else(|| anyhow!("token_command is empty"))?;

    let out = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .with_context(|| format!("running command: {cmd}"))?;

    if !out.status.success() {
        bail!(
            "token_command failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if token.is_empty() {
        bail!("token_command printed nothing on stdout");
    }
    Ok(token)
}
