//! The build-script half: stamping provenance in.
//!
//! Writes two things: `cargo:rustc-env=<PREFIX>_*` variables, and a Rust source
//! file in `OUT_DIR` defining a `STAMP` constant. The consumer includes the
//! file; see the crate docs for why an `env!` in this crate cannot work.
//!
//! Reading the environment is this module's entire job -- a build script's
//! input *is* its environment, and it runs single-threaded before anything
//! else exists. That is why the crate-wide ban is lifted here and nowhere else.
#![allow(clippy::disallowed_methods)]

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Stamper {
    prefix: String,
    tag_glob: Option<String>,
    out_file: String,
}

impl Stamper {
    /// `prefix` names the emitted variables: `"GSNZ"` gives `GSNZ_VERSION`.
    pub fn new(prefix: impl Into<String>) -> Stamper {
        Stamper {
            prefix: prefix.into(),
            tag_glob: None,
            out_file: "build_stamp.rs".into(),
        }
    }

    /// Which tags count as this project's releases, e.g.
    /// `"grocery-nz-cli/v*"`. In a monorepo every project's tags sit on the
    /// same commits, so without this a project reports its neighbour's release.
    pub fn tag_glob(mut self, glob: impl Into<String>) -> Stamper {
        self.tag_glob = Some(glob.into());
        self
    }

    pub fn out_file(mut self, name: impl Into<String>) -> Stamper {
        self.out_file = name.into();
        self
    }

    pub fn emit(self) -> std::io::Result<()> {
        let dir = PathBuf::from(env("CARGO_MANIFEST_DIR").unwrap_or_default());
        self.rerun_directives(&dir);

        // A release is built before the commit that bumps the manifest, so
        // CARGO_PKG_VERSION is still the previous release at that point.
        let version = env("DISPAT_NEW_VERSION")
            .or_else(|| env("CARGO_PKG_VERSION"))
            .unwrap_or_default();
        let commit = git(&dir, &["rev-parse", "--short=9", "HEAD"]).unwrap_or_default();
        let commit_date =
            git(&dir, &["log", "-1", "--format=%cd", "--date=short"]).unwrap_or_default();

        // The tag lands on the release commit *after* this runs, so `describe`
        // is empty during a release and dispat's value is the only one there is.
        let tag = env("DISPAT_TAG")
            .or_else(|| {
                let glob = self.tag_glob.as_deref()?;
                git(
                    &dir,
                    &["describe", "--tags", "--exact-match", "--match", glob],
                )
            })
            .unwrap_or_default();

        // CI says outright which repository this is; otherwise the origin
        // remote is the best answer available, which is how a build from a
        // fork correctly names the fork.
        let repo = env("GITHUB_REPOSITORY")
            .or_else(|| {
                git(&dir, &["remote", "get-url", "origin"])
                    .as_deref()
                    .and_then(slug)
            })
            .unwrap_or_default();

        let builder = if env("GITHUB_ACTIONS").as_deref() == Some("true") {
            "GitHub Actions"
        } else {
            ""
        };

        let rustc = Command::new(env("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            // "rustc 1.89.0 (hash date)" -- the number is the part anyone reads.
            .and_then(|v| v.split_whitespace().nth(1).map(str::to_string))
            .unwrap_or_default();

        let target = env("TARGET").unwrap_or_default();
        let profile = env("PROFILE").unwrap_or_default();

        let fields = [
            ("VERSION", version.as_str()),
            ("COMMIT", commit.as_str()),
            ("COMMIT_DATE", commit_date.as_str()),
            ("TAG", tag.as_str()),
            ("REPO", repo.as_str()),
            ("BUILDER", builder),
            ("RUSTC", rustc.as_str()),
            ("TARGET", target.as_str()),
            ("PROFILE", profile.as_str()),
        ];
        for (key, value) in fields {
            println!("cargo:rustc-env={}_{key}={value}", self.prefix);
        }

        let out_dir = PathBuf::from(env("OUT_DIR").ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "OUT_DIR is unset; this must run from a build script",
            )
        })?);
        std::fs::write(out_dir.join(&self.out_file), source(&fields))
    }

    fn rerun_directives(&self, dir: &Path) {
        // Naming any rerun-if-changed replaces cargo's default "rerun when a
        // file in the package changes", so the sources have to be re-declared
        // alongside the git ones. Without the git files a commit leaves a
        // stale sha behind.
        for path in ["build.rs", "src", "Cargo.toml"] {
            println!("cargo:rerun-if-changed={path}");
        }
        for key in [
            "DISPAT_NEW_VERSION",
            "DISPAT_TAG",
            "GITHUB_REPOSITORY",
            "GITHUB_ACTIONS",
        ] {
            println!("cargo:rerun-if-env-changed={key}");
        }
        for path in git_watch_paths(dir) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

/// The generated source. `::build_kit::Stamp` is spelled absolutely so the
/// include lands correctly whatever module the consumer puts it in.
fn source(fields: &[(&str, &str); 9]) -> String {
    let get = |name: &str| {
        fields
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| *v)
            .unwrap_or_default()
    };
    format!(
        "/// Generated by build-kit. Do not edit.\n\
         pub const STAMP: ::build_kit::Stamp = ::build_kit::Stamp {{\n\
         \x20   version: {:?},\n\
         \x20   commit: {:?},\n\
         \x20   commit_date: {:?},\n\
         \x20   tag: {:?},\n\
         \x20   repo: {:?},\n\
         \x20   builder: {:?},\n\
         \x20   rustc: {:?},\n\
         \x20   target: {:?},\n\
         \x20   profile: {:?},\n\
         }};\n",
        get("VERSION"),
        get("COMMIT"),
        get("COMMIT_DATE"),
        get("TAG"),
        get("REPO"),
        get("BUILDER"),
        get("RUSTC"),
        get("TARGET"),
        get("PROFILE"),
    )
}

/// A non-empty environment variable. CI passes unset ones through as `""`.
fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// `owner/name` out of a git remote, in either spelling git uses:
/// `git@github.com:owner/name.git` and `https://github.com/owner/name.git`.
pub fn slug(remote: &str) -> Option<String> {
    let rest = remote.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = rest.rsplit_once(':').map(|(_, r)| r).unwrap_or(rest);
    let mut parts = rest.rsplitn(3, '/');
    let name = parts.next()?;
    let owner = parts.next()?;
    (!name.is_empty() && !owner.is_empty()).then(|| format!("{owner}/{name}"))
}

/// Run git in the project directory, returning its trimmed stdout. `None` for
/// anything that is not a clean success: no git, no repository, a detached
/// state the command cannot answer for.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The git files whose contents move when HEAD moves: the HEAD pointer and the
/// branch ref it names.
fn git_watch_paths(dir: &Path) -> Vec<PathBuf> {
    let Some(git_dir) = git(dir, &["rev-parse", "--absolute-git-dir"]).map(PathBuf::from) else {
        return Vec::new();
    };
    let mut paths = vec![git_dir.join("HEAD")];
    if let Some(head) = std::fs::read_to_string(git_dir.join("HEAD"))
        .ok()
        .and_then(|h| h.strip_prefix("ref: ").map(|r| r.trim().to_string()))
    {
        paths.push(git_dir.join(head));
    }
    paths.into_iter().filter(|p| p.exists()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_slug_out_of_either_remote_spelling() {
        assert_eq!(
            slug("git@github.com:owner/name.git").as_deref(),
            Some("owner/name")
        );
        assert_eq!(
            slug("https://github.com/owner/name.git").as_deref(),
            Some("owner/name")
        );
        assert_eq!(
            slug("https://github.com/owner/name/").as_deref(),
            Some("owner/name")
        );
        assert_eq!(slug("not-a-remote"), None);
    }

    #[test]
    fn the_generated_source_is_a_valid_const_and_escapes_its_values() {
        let fields = [
            ("VERSION", "1.2.3"),
            ("COMMIT", "a1b2c3"),
            ("COMMIT_DATE", "2026-09-03"),
            ("TAG", ""),
            ("REPO", "owner/name"),
            ("BUILDER", ""),
            // A value with a quote in it must not break the generated file.
            ("RUSTC", r#"1.98.0 "odd""#),
            ("TARGET", "aarch64-apple-darwin"),
            ("PROFILE", "debug"),
        ];
        let text = source(&fields);
        assert!(text.contains("pub const STAMP: ::build_kit::Stamp"));
        assert!(text.contains(r#"version: "1.2.3""#));
        assert!(text.contains(r#"tag: """#), "empty stays empty: {text}");
        assert!(text.contains(r#"\"odd\""#), "quotes escaped: {text}");
    }
}
