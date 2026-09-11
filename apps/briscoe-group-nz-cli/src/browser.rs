//! Minting a captcha token in a real browser.
//!
//! Gigya's `accounts.login` refuses any request with no reCAPTCHA token, and
//! refuses it *before* it looks at the password -- measured against the live
//! site, a correct email and password with no token answers `400006, Invalid
//! CaptchaToken`. So there is no HTTP-only sign-in to find, whatever the TLS
//! fingerprint, and a browser has to be involved exactly once.
//!
//! **Only once, and only for the token.** The browser is given the storefront
//! origin and nothing else: no email, no password. It runs Google's script on
//! the site's own origin -- a v3 token is bound to both the site key and the
//! page -- and hands back the string. This program then makes the sign-in
//! request itself, which keeps credentials out of a subprocess and lets it ask
//! for a session that outlives a browser tab, which the site's own login never
//! does. Everything after that sign-in refreshes with no browser at all.
//!
//! That browser is not this program's to ship. [`captcha`] drives one that is
//! already installed, through a script embedded in this binary, and says
//! plainly what is missing when there is none -- because a browser engine is a
//! hundred megabytes and almost none of this tool needs one.

use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{AppError, AppResult};

/// The driver, embedded so the binary is still one file.
const SCRIPT: &str = include_str!("browser/captcha.py");

/// What to say when there is no browser to drive.
///
/// A constant because it is the failure most people will meet, and it has to
/// carry two things: how to get a browser, and the way in that needs none.
const MISSING_BROWSER: &str = "signing in needs a browser, and `camoufox` was not found on PATH. \
     Install it (`uv tool install \"camoufox[geoip]\"` then `camoufox fetch`), \
     or use `bgnz auth token` instead";

/// What the script hands back.
#[derive(Debug, Deserialize)]
pub struct Captcha {
    pub captcha_token: String,
    /// Carried for `--debug` and for a bug report; nothing depends on it.
    #[serde(default)]
    pub site_key: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
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
    let launcher = which("camoufox").ok_or_else(|| AppError::browser(MISSING_BROWSER))?;
    let text = std::fs::read_to_string(&launcher)?;
    let shebang = text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("#!"))
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            AppError::browser(format!(
                "{} is not a script naming its interpreter; set BGNZ_BROWSER_PYTHON",
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

/// Open the storefront in a browser and come back with a captcha token.
pub async fn captcha(
    python: Option<&str>,
    script_dir: &std::path::Path,
    origin: &str,
    screen_set: Option<&str>,
    start_screen: Option<&str>,
    headless: bool,
) -> AppResult<Captcha> {
    let python = interpreter(python)?;

    // Written out each run rather than cached: the script belongs to this
    // build of the binary, and a stale copy from an older one would be worse
    // than a rewrite that costs nothing.
    std::fs::create_dir_all(script_dir)?;
    let script = script_dir.join("browser-captcha.py");
    std::fs::write(&script, SCRIPT)?;

    let mut command = tokio::process::Command::new(&python);
    command
        .arg(&script)
        .env("BGNZ_ORIGIN", origin)
        // Gigya loads its captcha when the sign-in screen opens rather than on
        // page load, so the script has to be able to open one. The names come
        // from the storefront's own config rather than being baked into the
        // script.
        .env("BGNZ_SCREEN_SET", screen_set.unwrap_or_default())
        .env("BGNZ_START_SCREEN", start_screen.unwrap_or_default())
        // The script reads "unset or a denial" as "show a window", which is
        // the same convention the rest of this program uses for flags.
        .env("BGNZ_HEADLESS", if headless { "1" } else { "" })
        .stdout(std::process::Stdio::piped())
        // Inherited, not captured: its progress lines are the only sign of
        // life during a run that can take half a minute.
        .stderr(std::process::Stdio::inherit());

    let output = command.output().await.map_err(|e| {
        AppError::browser(format!(
            "could not run {} to drive the browser: {e}",
            python.display()
        ))
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().last().unwrap_or_default().trim();

    if let Ok(failure) = serde_json::from_str::<BrowserError>(line) {
        return Err(AppError::browser(failure.error));
    }
    serde_json::from_str::<Captcha>(line).map_err(|_| {
        AppError::browser(if line.is_empty() {
            "the browser exited without saying anything".to_string()
        } else {
            format!("the browser answered with something unreadable: {line}")
        })
    })
}

/// Whether there is a browser to drive, for `doctor` to report.
///
/// Cheap and side-effect free: it looks for the launcher, not for a working
/// Firefox, because the expensive half only matters when a sign-in is actually
/// attempted.
pub fn available(python: Option<&str>) -> Option<String> {
    if let Some(path) = python {
        return Some(path.to_string());
    }
    which("camoufox").map(|p| p.display().to_string())
}

/// The cookie a signed-in browser keeps the login token in.
///
/// The whole of `auth token`'s instructions. Gigya files its session under
/// `glt_<apiKey>` on the site's own domain, so the credential can be copied out
/// of devtools' cookie list -- no network tab, no storage spelunking.
pub fn cookie_name(api_key: &str) -> String {
    format!("glt_{api_key}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cookie_to_copy_is_named_for_the_fascias_own_key() {
        // Different per fascia, which is the point: copying the Briscoes
        // cookie into the Rebel Sport slot would fail in a way that reads as a
        // bad token rather than as the wrong one.
        assert_eq!(
            cookie_name("4_R5DCbpg6mrfoot48AR2Wcg"),
            "glt_4_R5DCbpg6mrfoot48AR2Wcg"
        );
        assert_eq!(
            cookie_name("4_27oU1279wfzlXMCv-nEj0w"),
            "glt_4_27oU1279wfzlXMCv-nEj0w"
        );
    }

    #[test]
    fn a_missing_browser_names_both_ways_forward() {
        // The message is the failure most people will meet, so it has to carry
        // how to install one *and* that there is a path that needs none.
        assert!(MISSING_BROWSER.contains("camoufox"));
        assert!(MISSING_BROWSER.contains("bgnz auth token"));
    }

    #[test]
    fn an_explicit_interpreter_is_taken_as_given() {
        let path = interpreter(Some("/usr/bin/python3")).expect("an explicit path is honoured");
        assert_eq!(path, PathBuf::from("/usr/bin/python3"));
    }
}
