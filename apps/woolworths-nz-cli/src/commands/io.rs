//! Small input and output helpers shared by the command modules.

use anyhow::{bail, Context, Result};
use std::time::Duration;

/// Ask for a line on stdin. Refuses when there is no terminal, so a script
/// gets a clear error instead of hanging on a prompt nobody will see.
pub(super) fn prompt(message: &str) -> Result<String> {
    use std::io::{stdin, stdout, IsTerminal, Write};
    if !stdin().is_terminal() {
        bail!("{message}required, but there is no terminal to prompt on; pass it as a flag");
    }
    print!("{message}");
    stdout().flush().ok();
    let mut line = String::new();
    stdin().read_line(&mut line).context("reading input")?;
    let line = line.trim().to_string();
    if line.is_empty() {
        bail!("nothing entered");
    }
    Ok(line)
}

pub(super) fn print_json(value: &serde_json::Value) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(e) => eprintln!("could not serialise output: {e}"),
    }
}

pub(super) fn human_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    format!("{}h {}m", mins / 60, mins % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_the_way_people_say_them() {
        assert_eq!(human_duration(Duration::from_secs(45)), "45s");
        assert_eq!(human_duration(Duration::from_secs(120)), "2m");
        assert_eq!(human_duration(Duration::from_secs(3900)), "1h 5m");
    }
}
