//! Stamps the binary with where it came from.
//!
//! `fsnz -V` has to answer "which build of this is running?" for a binary that
//! could have arrived three ways: `cargo build` in a working tree, `cargo
//! install` from a checkout, or `fsnz update` pulling a release tarball. The
//! git facts below cover the first two; the third leaves a marker file that
//! `crate::build` reads at runtime.
//!
//! Everything here degrades to an empty string. A source tarball with no `.git`
//! and no `git` on PATH still builds -- it just reports less.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // Naming any rerun-if-changed replaces cargo's default "rerun when a file
    // in the package changes", so the sources have to be re-declared alongside
    // the git files. Without the git ones a commit would leave a stale sha;
    // without the source ones an edit would leave a stale dirty flag.
    println!("cargo:rerun-if-changed=build.rs");
    // A release build is stamped from the workflow's environment, so a change
    // of repository or of CI has to re-stamp.
    println!("cargo:rerun-if-env-changed=GITHUB_REPOSITORY");
    println!("cargo:rerun-if-env-changed=GITHUB_ACTIONS");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    for path in git_watch_paths(&dir) {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let commit = git(&dir, &["rev-parse", "--short=9", "HEAD"]);
    let commit_date = git(&dir, &["log", "-1", "--format=%cd", "--date=short"]);

    // Scoped to this project's directory. In a monorepo an unrelated project's
    // uncommitted edits say nothing about how this binary was built.
    let dirty = git(&dir, &["status", "--porcelain", "--", "."])
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    // Releases are tagged `<project>/vX.Y.Z`, so an exact match here means this
    // is a build of a published release rather than of some commit near one.
    let tag = git(
        &dir,
        &[
            "describe",
            "--tags",
            "--exact-match",
            "--match",
            "foodstuffs-nz-cli/v*",
        ],
    );

    // Which repository this came out of. GitHub Actions says so outright;
    // otherwise the origin remote is the best answer available, which is how a
    // build from a fork ends up naming the fork.
    let repo = std::env::var("GITHUB_REPOSITORY")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| {
            git(&dir, &["remote", "get-url", "origin"])
                .as_deref()
                .and_then(slug)
        });

    // Only ever set by the release workflow, so it is the one thing that
    // separates "built from the release tag" from "built on a laptop that
    // happened to have the tag checked out".
    let builder =
        (std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")).then_some("GitHub Actions");

    let rustc = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        // "rustc 1.89.0 (hash date)" -- the version number alone is the part
        // anyone reads.
        .and_then(|v| v.split_whitespace().nth(1).map(str::to_string));

    emit("FSNZ_COMMIT", commit.as_deref());
    emit("FSNZ_COMMIT_DATE", commit_date.as_deref());
    emit("FSNZ_TAG", tag.as_deref());
    emit("FSNZ_REPO", repo.as_deref());
    emit("FSNZ_BUILDER", builder);
    emit("FSNZ_DIRTY", Some(if dirty { "true" } else { "false" }));
    emit("FSNZ_RUSTC", rustc.as_deref());
    emit("FSNZ_TARGET", std::env::var("TARGET").ok().as_deref());
    emit("FSNZ_PROFILE", std::env::var("PROFILE").ok().as_deref());
}

/// `owner/name` out of a git remote, in either of the two spellings git uses:
/// `git@github.com:owner/name.git` and `https://github.com/owner/name.git`.
fn slug(remote: &str) -> Option<String> {
    let rest = remote.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = rest.rsplit_once(':').map(|(_, r)| r).unwrap_or(rest);
    let mut parts = rest.rsplitn(3, '/');
    let name = parts.next()?;
    let owner = parts.next()?;
    (!name.is_empty() && !owner.is_empty()).then(|| format!("{owner}/{name}"))
}

fn emit(key: &str, value: Option<&str>) {
    println!("cargo:rustc-env={key}={}", value.unwrap_or(""));
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

/// The git files whose contents move when HEAD moves: the HEAD pointer, the
/// branch ref it names, and the index (which is what `status` compares).
fn git_watch_paths(dir: &Path) -> Vec<PathBuf> {
    let Some(git_dir) = git(dir, &["rev-parse", "--absolute-git-dir"]).map(PathBuf::from) else {
        return Vec::new();
    };
    let mut paths = vec![git_dir.join("HEAD"), git_dir.join("index")];
    if let Some(head) = std::fs::read_to_string(git_dir.join("HEAD"))
        .ok()
        .and_then(|h| h.strip_prefix("ref: ").map(|r| r.trim().to_string()))
    {
        paths.push(git_dir.join(head));
    }
    paths.into_iter().filter(|p| p.exists()).collect()
}
