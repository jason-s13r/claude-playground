//! What this binary is, and where it came from.
//!
//! Half of the answer is stamped in at compile time by `build.rs`; the other
//! half is only knowable at runtime -- the path the binary is running from, and
//! whether [`crate::update`] is what put it there.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Paths;

/// Stamped by `build.rs`: a release is compiled before the manifest bump.
pub const VERSION: &str = env!("WWNZ_VERSION");

/// Empty when the build had no git to ask -- a source tarball, or a machine
/// without git installed.
const COMMIT: &str = env!("WWNZ_COMMIT");
const COMMIT_DATE: &str = env!("WWNZ_COMMIT_DATE");
/// The release tag HEAD sat exactly on, when it sat on one.
const TAG: &str = env!("WWNZ_TAG");
/// The repository the build came out of: `GITHUB_REPOSITORY` in CI, the origin
/// remote otherwise. Empty when neither could be read.
const REPO: &str = env!("WWNZ_REPO");
/// Set only by the release workflow.
const BUILDER: &str = env!("WWNZ_BUILDER");
const RUSTC: &str = env!("WWNZ_RUSTC");
const TARGET: &str = env!("WWNZ_TARGET");
pub const PROFILE: &str = env!("WWNZ_PROFILE");

fn some(s: &'static str) -> Option<&'static str> {
    (!s.is_empty()).then_some(s)
}

/// `-V`: one line, enough to tell two builds of the same version apart.
pub fn short_version() -> &'static str {
    static CELL: OnceLock<String> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut parts = Vec::new();
        // A build of a release tag is the one thing worth saying before the
        // commit: it is the difference between a published binary and one
        // somebody made. `--version` names the repository and the builder.
        if let Some(tag) = some(TAG) {
            parts.push(tag.to_string());
        }
        let commit = [
            some(COMMIT).map(str::to_string),
            some(COMMIT_DATE).map(str::to_string),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        if !commit.is_empty() {
            parts.push(commit);
        }
        if let Some(profile) = some(PROFILE) {
            parts.push(format!("{profile} build"));
        }
        if parts.is_empty() {
            VERSION.to_string()
        } else {
            format!("{VERSION} ({})", parts.join(", "))
        }
    })
}

/// `--version`: the whole provenance, including the two things only the running
/// process knows -- which file it is, and how that file got there.
pub fn long_version() -> &'static str {
    static CELL: OnceLock<String> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut out = String::from(VERSION);
        let mut line = |label: &str, value: String| {
            out.push_str(&format!("\n{label:<11}{value}"));
        };

        if let Some(commit) = some(COMMIT).map(str::to_string) {
            line(
                "commit",
                match some(COMMIT_DATE) {
                    Some(date) => format!("{commit} ({date})"),
                    None => commit,
                },
            );
        }
        let mut source = vec![match some(REPO) {
            Some(repo) => repo.to_string(),
            None => "an unidentified repository".to_string(),
        }];
        source.push(match some(TAG) {
            Some(tag) => format!("release tag {tag}"),
            None => "no release tag".to_string(),
        });
        line("source", source.join(", "));
        line(
            "built by",
            match some(BUILDER) {
                Some(builder) => format!("{builder}, from the release workflow"),
                // Nobody but the release workflow stamps a builder, so this is
                // a binary someone compiled themselves -- possibly the person
                // reading this, possibly not.
                None => "hand, not by the release workflow".to_string(),
            },
        );

        let mut build: Vec<String> = [some(PROFILE), some(TARGET)]
            .into_iter()
            .flatten()
            .map(str::to_string)
            .collect();
        if let Some(rustc) = some(RUSTC) {
            build.push(format!("rustc {rustc}"));
        }
        if !build.is_empty() {
            line("build", build.join(", "));
        }
        if let Some(path) = exe_path() {
            line("binary", path.display().to_string());
        }
        // Only when there is a record. A hand-built binary and a `cargo
        // install` one are indistinguishable from in here, so the line is
        // left out rather than filled with a guess.
        if let Some(i) = Install::current() {
            line(
                "installed",
                format!(
                    "by `wwnz update` from {} on {}",
                    i.tag,
                    iso_date(i.installed_at)
                ),
            );
        }
        out
    })
}

/// The build stamp as data, for `--json` consumers.
pub fn json() -> serde_json::Value {
    let field = |v: &'static str| {
        some(v)
            .map(|s| serde_json::Value::String(s.to_string()))
            .unwrap_or(serde_json::Value::Null)
    };
    serde_json::json!({
        "version": VERSION,
        "commit": field(COMMIT),
        "commit_date": field(COMMIT_DATE),
        "tag": field(TAG),
        "repo": field(REPO),
        "builder": field(BUILDER),
        "profile": field(PROFILE),
        "target": field(TARGET),
        "rustc": field(RUSTC),
        "binary": exe_path().map(|p| p.display().to_string()),
        "installed": Install::current(),
    })
}

/// The running binary, with symlinks resolved: `wwnz update` replaces the real
/// file, not the link someone put on their PATH.
pub fn exe_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.canonicalize().unwrap_or(exe))
}

/// The record `wwnz update` leaves behind, so a later `wwnz --version` can say
/// where the binary came from.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Install {
    pub version: String,
    pub tag: String,
    /// The release page, for anyone wanting the notes.
    pub url: String,
    pub asset: String,
    /// Which file was replaced. A marker naming some other path belongs to a
    /// different copy of wwnz and says nothing about this one.
    pub path: PathBuf,
    pub installed_at: u64,
}

impl Install {
    fn file(paths: &Paths) -> PathBuf {
        paths.state_dir.join("install.json")
    }

    pub fn load(paths: &Paths) -> Option<Install> {
        let text = std::fs::read_to_string(Install::file(paths)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, paths: &Paths) -> anyhow::Result<()> {
        use anyhow::Context;
        std::fs::create_dir_all(&paths.state_dir)
            .with_context(|| format!("creating {}", paths.state_dir.display()))?;
        let file = Install::file(paths);
        let text = serde_json::to_string_pretty(self).context("serialising install record")?;
        std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))
    }

    /// The record for *this* binary, if there is one.
    fn current() -> Option<Install> {
        let paths = Paths::resolve().ok()?;
        let install = Install::load(&paths)?;
        (Some(&install.path) == exe_path().as_ref()).then_some(install)
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `YYYY-MM-DD` in UTC. Days-to-civil, so no date crate is needed for the one
/// date this tool ever formats.
pub fn iso_date(epoch_secs: u64) -> String {
    let days = (epoch_secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_come_back_as_utc_days() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(1_756_512_000), "2025-08-30");
        // The last second of a day still belongs to that day.
        assert_eq!(iso_date(1_756_598_399), "2025-08-30");
        assert_eq!(iso_date(1_756_598_400), "2025-08-31");
        // A leap day, which is where naive day arithmetic goes wrong.
        assert_eq!(iso_date(1_709_164_800), "2024-02-29");
    }

    #[test]
    fn the_short_version_always_leads_with_the_version() {
        assert!(
            short_version().starts_with(VERSION),
            "got: {}",
            short_version()
        );
        assert!(
            long_version().starts_with(VERSION),
            "got: {}",
            long_version()
        );
    }
}
