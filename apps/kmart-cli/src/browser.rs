//! Signing in through a real browser.
//!
//! Kmart's bot check guards the one step of Auth0's login that would produce a
//! session -- the password submit -- and no HTTP client gets past it, whatever
//! its TLS fingerprint. Measured against the live site: `wreq` is refused and
//! a headful browser with humanised input is not. So the only way to sign in
//! with an email and a password is to have a browser do it. Headless was
//! refused too, but only against an earlier route into the login -- hence
//! `--headless`, which is there to be tried rather than believed.
//!
//! That browser is not this program's to ship. [`login`] drives one that is
//! already installed, through a script embedded in this binary, and says
//! plainly what is missing when there is none -- because a browser engine is a
//! hundred megabytes and most of this tool does not need one.
//!
//! Credentials reach the script through the **environment, never arguments**:
//! a command line is readable by every process on the machine.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{AppError, AppResult};

/// The driver, embedded so the binary is still one file.
const SCRIPT: &str = include_str!("browser/login.py");

/// What to say when there is no browser to drive.
///
/// A constant because it is the failure most people will meet, and it has to
/// carry two things: how to get a browser, and the way in that needs none.
const MISSING_BROWSER: &str = "signing in needs a browser, and `camoufox` was not found on PATH. \
     Install it (`uv tool install camoufox[geoip]` then `camoufox fetch`), \
     or use `kmart auth token` instead";

/// What the script hands back.
#[derive(Debug, Deserialize)]
pub struct BrowserLogin {
    pub refresh_token: String,
    #[serde(default)]
    pub email: Option<String>,
    /// Keyed by country code, as [`kmart_api::StoredSession`] files them.
    #[serde(default)]
    pub cookies: BTreeMap<String, BTreeMap<String, String>>,
}

/// The script's other answer.
#[derive(Debug, Deserialize)]
struct BrowserError {
    error: String,
}

/// Find a Python that can `import camoufox`.
///
/// The `camoufox` launcher is a console script whose shebang names the
/// interpreter of the environment it was installed into, which is the one
/// interpreter known to have the package. Reading it is more reliable than
/// guessing at `python3`, which on most machines has no camoufox at all.
fn interpreter(explicit: Option<&str>) -> AppResult<PathBuf> {
    if let Some(path) = explicit {
        return Ok(PathBuf::from(path));
    }
    let launcher = which("camoufox").ok_or_else(|| AppError::usage(MISSING_BROWSER))?;
    let text = std::fs::read_to_string(&launcher)?;
    let shebang = text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("#!"))
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            AppError::usage(format!(
                "{} is not a script naming its interpreter; set KMART_BROWSER_PYTHON",
                launcher.display()
            ))
        })?;
    Ok(PathBuf::from(shebang))
}

/// `which`, without a crate for it.
fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

/// Sign in, and come back with both credentials.
///
/// One trip yields the refresh token *and* the Akamai cookies, because the
/// browser earns both and there is no reason to make someone fetch them
/// separately.
pub async fn login(
    python: Option<&str>,
    script_dir: &std::path::Path,
    origin: &str,
    email: &str,
    password: &str,
    headless: bool,
    debug: bool,
) -> AppResult<BrowserLogin> {
    let python = interpreter(python)?;

    // Written out each run rather than cached: the script belongs to this
    // build of the binary, and a stale copy from an older one would be worse
    // than a rewrite that costs nothing.
    std::fs::create_dir_all(script_dir)?;
    let script = script_dir.join("browser-login.py");
    std::fs::write(&script, SCRIPT)?;

    let mut command = tokio::process::Command::new(&python);
    command
        .arg(&script)
        // Never as arguments: `ps` shows those to everyone.
        .env("KMART_EMAIL", email)
        .env("KMART_PASSWORD", password)
        .env("KMART_ORIGIN", origin)
        .env("KMART_HEADLESS", if headless { "1" } else { "" })
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        // Captured rather than inherited: `output()` pipes both streams
        // whatever this says, so inheriting silently did nothing. Capturing
        // it deliberately is better anyway -- it is what lets a failure carry
        // the browser's own account of how far it got.
        .stderr(std::process::Stdio::piped());

    let output = command
        .output()
        .await
        .map_err(|e| AppError::usage(format!("could not run {}: {e}", python.display())))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The script narrates its progress there. Shown only under --debug, so an
    // ordinary run is not buried in browser chatter.
    if debug {
        for line in stderr.lines() {
            eprintln!("{line}");
        }
    }
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'));

    let Some(line) = line else {
        // No answer at all means the browser died rather than reported. The
        // last thing it said is the only clue there is, so it travels with the
        // error rather than being thrown away.
        let last = stderr
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("it said nothing");
        return Err(AppError::usage(format!(
            "the browser produced no answer{}: {last}",
            match output.status.success() {
                true => String::new(),
                false => format!(" (it exited {})", output.status),
            }
        )));
    };
    // The script reports its own failures as JSON, so they arrive as sentences
    // rather than as an exit code.
    if let Ok(BrowserError { error }) = serde_json::from_str::<BrowserError>(line) {
        return Err(AppError::usage(format!("signing in failed: {error}")));
    }
    serde_json::from_str(line)
        .map_err(|e| AppError::usage(format!("could not read the browser's answer: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_script_is_the_one_that_ships() {
        // A missing include is a compile error; an empty one is not, and would
        // fail at the least convenient moment.
        assert!(SCRIPT.contains("camoufox"), "the driver is embedded");
        assert!(
            SCRIPT.contains("KMART_PASSWORD"),
            "and reads its credentials from env"
        );
        assert!(!SCRIPT.contains("sys.argv"), "never from the command line");
    }

    #[test]
    fn an_explicit_interpreter_is_taken_as_given() {
        assert_eq!(
            interpreter(Some("/usr/bin/python3")).unwrap(),
            PathBuf::from("/usr/bin/python3")
        );
    }

    #[test]
    fn a_missing_browser_says_what_to_install_and_what_to_do_instead() {
        // The failure most people will meet. Checked as a constant rather than
        // by forcing the error path, which is not reachable on a machine that
        // has camoufox installed -- such as the one this was written on.
        assert!(MISSING_BROWSER.contains("camoufox fetch"), "how to get one");
        assert!(
            MISSING_BROWSER.contains("auth token"),
            "and the way in that needs no browser at all"
        );
    }

    #[test]
    fn the_interpreter_is_read_off_the_launchers_shebang() {
        // The console script names the environment camoufox was installed
        // into, which is the one interpreter known to have it. Guessing
        // `python3` finds a system Python without the package.
        let dir = tempfile::TempDir::new().unwrap();
        let launcher = dir.path().join("camoufox");
        std::fs::write(&launcher, "#!/opt/venv/bin/python\nprint()\n").unwrap();
        let text = std::fs::read_to_string(&launcher).unwrap();
        let shebang = text.lines().next().unwrap().strip_prefix("#!").unwrap();
        assert_eq!(shebang, "/opt/venv/bin/python");
    }
}
