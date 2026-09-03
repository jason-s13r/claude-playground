//! The build stamp, as data.
//!
//! Everything here is a method on a plain struct rather than a function over
//! `env!` constants, which is why the version strings below can be tested
//! without compiling a binary to test them against.

use serde::Serialize;

/// Where a binary came from. Written by [`crate::emit::Stamper`] into the
/// consumer's `OUT_DIR` and included from there.
///
/// Every field degrades to `""`. A source tarball with no `.git` and no `git`
/// on PATH still builds -- it just reports less.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Stamp {
    /// Stamped from the release, because a release is compiled *before* the
    /// commit that bumps the manifest -- `CARGO_PKG_VERSION` is stale there.
    pub version: &'static str,
    pub commit: &'static str,
    pub commit_date: &'static str,
    /// The release tag HEAD sat exactly on, when it sat on one.
    pub tag: &'static str,
    pub repo: &'static str,
    /// Set only by a release workflow, so it is what separates a published
    /// binary from one somebody built on a laptop that had the tag checked out.
    pub builder: &'static str,
    pub rustc: &'static str,
    pub target: &'static str,
    pub profile: &'static str,
}

fn some(s: &'static str) -> Option<&'static str> {
    (!s.is_empty()).then_some(s)
}

impl Stamp {
    /// A stamp with nothing in it, for tests and for a build with no script.
    pub const EMPTY: Stamp = Stamp {
        version: "0.0.0",
        commit: "",
        commit_date: "",
        tag: "",
        repo: "",
        builder: "",
        rustc: "",
        target: "",
        profile: "",
    };

    /// `-V`: one line, enough to tell two builds of one version apart.
    pub fn short_version(&self) -> String {
        let mut parts = Vec::new();
        // A build of a release tag is the one thing worth saying before the
        // commit: it separates a published binary from one somebody made.
        if let Some(tag) = some(self.tag) {
            parts.push(tag.to_string());
        }
        let commit = [some(self.commit), some(self.commit_date)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        if !commit.is_empty() {
            parts.push(commit);
        }
        if let Some(profile) = some(self.profile) {
            parts.push(format!("{profile} build"));
        }
        if parts.is_empty() {
            self.version.to_string()
        } else {
            format!("{} ({})", self.version, parts.join(", "))
        }
    }

    /// `--version`: the whole provenance, plus the two things only the running
    /// process knows -- which file it is, and how that file got there.
    pub fn long_version(&self, binary: Option<&str>, installed: Option<&InstallNote>) -> String {
        let mut out = String::from(self.version);
        let mut line = |label: &str, value: String| {
            out.push_str(&format!("\n{label:<11}{value}"));
        };

        if let Some(commit) = some(self.commit) {
            line(
                "commit",
                match some(self.commit_date) {
                    Some(date) => format!("{commit} ({date})"),
                    None => commit.to_string(),
                },
            );
        }
        line(
            "source",
            format!(
                "{}, {}",
                some(self.repo).unwrap_or("an unidentified repository"),
                match some(self.tag) {
                    Some(tag) => format!("release tag {tag}"),
                    None => "no release tag".to_string(),
                }
            ),
        );
        line(
            "built by",
            match some(self.builder) {
                Some(builder) => format!("{builder}, from the release workflow"),
                // Nobody but the release workflow stamps a builder, so this is
                // a binary someone compiled themselves -- possibly the person
                // reading it, possibly not.
                None => "hand, not by the release workflow".to_string(),
            },
        );

        let mut build: Vec<String> = [some(self.profile), some(self.target)]
            .into_iter()
            .flatten()
            .map(str::to_string)
            .collect();
        if let Some(rustc) = some(self.rustc) {
            build.push(format!("rustc {rustc}"));
        }
        if !build.is_empty() {
            line("build", build.join(", "));
        }
        if let Some(path) = binary {
            line("binary", path.to_string());
        }
        // Only when there is a record. A hand-built binary and a `cargo
        // install` one are indistinguishable from in here, so the line is left
        // out rather than filled with a guess.
        if let Some(note) = installed {
            line(
                "installed",
                format!(
                    "by `{} update` from {} on {}",
                    note.tool,
                    note.tag,
                    crate::date::iso_date(note.installed_at)
                ),
            );
        }
        out
    }

    /// The stamp for `--json` consumers. Empty fields become `null` rather than
    /// `""`, so "not known" is distinguishable from "known to be empty".
    pub fn json(&self) -> serde_json::Value {
        let field = |v: &'static str| match some(v) {
            Some(s) => serde_json::Value::String(s.to_string()),
            None => serde_json::Value::Null,
        };
        serde_json::json!({
            "version": self.version,
            "commit": field(self.commit),
            "commit_date": field(self.commit_date),
            "tag": field(self.tag),
            "repo": field(self.repo),
            "builder": field(self.builder),
            "profile": field(self.profile),
            "target": field(self.target),
            "rustc": field(self.rustc),
        })
    }
}

/// Just enough of an install record for [`Stamp::long_version`] to mention it,
/// so the stamp does not need the runtime feature to render.
pub struct InstallNote<'a> {
    pub tool: &'a str,
    pub tag: &'a str,
    pub installed_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASED: Stamp = Stamp {
        version: "1.2.3",
        commit: "a1b2c3d4e",
        commit_date: "2026-09-03",
        tag: "grocery-nz-cli/v1.2.3",
        repo: "jason-s13r/claude-playground",
        builder: "GitHub Actions",
        rustc: "1.98.0",
        target: "aarch64-apple-darwin",
        profile: "release",
    };

    #[test]
    fn the_version_always_comes_first() {
        assert!(RELEASED.short_version().starts_with("1.2.3"));
        assert!(RELEASED.long_version(None, None).starts_with("1.2.3"));
        assert!(Stamp::EMPTY.short_version().starts_with("0.0.0"));
    }

    #[test]
    fn a_bare_stamp_reports_only_the_version() {
        assert_eq!(Stamp::EMPTY.short_version(), "0.0.0");
    }

    #[test]
    fn the_short_version_leads_with_the_release_tag() {
        let text = RELEASED.short_version();
        assert!(text.contains("grocery-nz-cli/v1.2.3"), "{text}");
        assert!(text.contains("a1b2c3d4e"), "{text}");
        assert!(text.contains("release build"), "{text}");
    }

    #[test]
    fn an_unstamped_builder_is_named_as_hand_built_not_left_blank() {
        let mut stamp = RELEASED;
        stamp.builder = "";
        let text = stamp.long_version(None, None);
        assert!(text.contains("hand, not by the release workflow"), "{text}");
    }

    #[test]
    fn an_unknown_repository_says_so_rather_than_printing_nothing() {
        let mut stamp = RELEASED;
        stamp.repo = "";
        stamp.tag = "";
        let text = stamp.long_version(None, None);
        assert!(text.contains("an unidentified repository"), "{text}");
        assert!(text.contains("no release tag"), "{text}");
    }

    #[test]
    fn an_install_record_is_reported_with_its_date() {
        let note = InstallNote {
            tool: "gsnz",
            tag: "grocery-nz-cli/v1.2.3",
            installed_at: 1_756_512_000,
        };
        let text = RELEASED.long_version(Some("/usr/local/bin/gsnz"), Some(&note));
        assert!(text.contains("`gsnz update`"), "{text}");
        assert!(text.contains("2025-08-30"), "{text}");
        assert!(text.contains("/usr/local/bin/gsnz"), "{text}");
    }

    #[test]
    fn json_distinguishes_unknown_from_empty() {
        let value = Stamp::EMPTY.json();
        assert_eq!(value["version"], "0.0.0");
        assert!(value["commit"].is_null(), "unknown is null, not \"\"");
        assert_eq!(RELEASED.json()["tag"], "grocery-nz-cli/v1.2.3");
    }
}
