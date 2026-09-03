//! Asking for input, and one duration formatter.
//!
//! Everything here writes its prompt to **stderr**. A prompt on stdout would
//! be inside the document `--json` is producing.

use std::io::{self, IsTerminal, Write};
use std::time::Duration;

/// Ask for a line on stdin.
///
/// Refuses when there is no terminal, so a script gets a clear error instead of
/// hanging on a prompt nobody will ever see.
pub fn prompt(message: &str) -> io::Result<String> {
    if !io::stdin().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{message} is required, but there is no terminal to ask on; pass it as a flag"),
        ));
    }
    eprint!("{message}: ");
    io::stderr().flush().ok();
    read_line(message)
}

/// Ask for a line, accepting a piped one.
///
/// Unlike [`prompt`], which refuses without a terminal: a one-time code cannot
/// be passed as a flag, because it does not exist until the request demanding
/// it has already been made.
pub fn prompt_or_stdin(message: &str) -> io::Result<String> {
    if io::stdin().is_terminal() {
        return prompt(message);
    }
    read_line(message)
}

pub fn prompt_password(message: &str) -> io::Result<String> {
    if !io::stdin().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{message} is required, but there is no terminal to ask on"),
        ));
    }
    let value = rpassword::prompt_password(format!("{message}: "))?;
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nothing entered",
        ));
    }
    Ok(value)
}

/// A yes/no question. Anything but an explicit yes is a no.
pub fn confirm(message: &str) -> io::Result<bool> {
    if !io::stdin().is_terminal() {
        return Ok(false);
    }
    eprint!("{message} [y/N]: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn read_line(message: &str) -> io::Result<String> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let line = line.trim().to_string();
    if line.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{message} is required; nothing was entered"),
        ));
    }
    Ok(line)
}

/// A duration the way a person would say it.
pub fn human_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let (hours, mins) = (mins / 60, mins % 60);
    if hours < 24 {
        return format!("{hours}h {mins}m");
    }
    format!("{}d {}h", hours / 24, hours % 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_the_way_people_say_them() {
        assert_eq!(human_duration(Duration::from_secs(45)), "45s");
        assert_eq!(human_duration(Duration::from_secs(120)), "2m");
        assert_eq!(human_duration(Duration::from_secs(3900)), "1h 5m");
        assert_eq!(human_duration(Duration::from_secs(90_000)), "1d 1h");
    }

    #[test]
    fn zero_is_seconds_not_empty() {
        assert_eq!(human_duration(Duration::from_secs(0)), "0s");
    }
}
