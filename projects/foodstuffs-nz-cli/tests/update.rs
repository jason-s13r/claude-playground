//! End-to-end: `fsnz update` against a stand-in GitHub.
//!
//! The install tests run a *copy* of the binary out of a temp directory. That
//! is the whole point of the command -- it replaces the file it is running
//! from -- and doing it to the one cargo just built would break every other
//! test in the run.

mod support;

use serde_json::json;
use support::*;

const CURRENT: &str = env!("FSNZ_VERSION");
const NEWER: &str = "9.9.0";

/// A release of `NEWER` with a binary for this host and a matching SHA256SUMS,
/// served from `gh`. Returns the payload the tarball carries.
async fn mount_newer_release(gh: &Github, payload: &[u8]) -> Vec<String> {
    let names = host_asset_names(NEWER);
    let archive = tarball("fsnz", payload);

    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for name in &names {
        gh.asset(name, archive.clone()).await;
        files.push((name.clone(), archive.clone()));
    }
    gh.asset("SHA256SUMS", sha256sums(&files)).await;

    let mut assets = names.clone();
    assets.push("SHA256SUMS".to_string());
    gh.releases(json!([
        gh.release(&format!("foodstuffs-nz-cli/v{NEWER}"), &assets)
    ]))
    .await;
    assets
}

/// The binary under a path of its own, so replacing it harms nothing.
fn binary_copy(dir: &std::path::Path) -> std::path::PathBuf {
    let src = assert_cmd::cargo::cargo_bin("fsnz");
    let dest = dir.join("fsnz");
    std::fs::copy(&src, &dest).expect("copying the binary");
    dest
}

#[tokio::test]
async fn check_finds_the_newest_release_of_this_project_alone() {
    let f = Fixture::start().await;
    let gh = Github::start().await;
    let assets = host_asset_names(NEWER);

    gh.releases(json!([
        // Another project in the monorepo, released far ahead of this one.
        // GitHub's own `releases/latest` would hand back exactly this.
        gh.release("other-project/v42.0.0", &[]),
        gh.release("foodstuffs-nz-cli/v0.0.9", &[]),
        gh.release(&format!("foodstuffs-nz-cli/v{NEWER}"), &assets),
        // Flagged prerelease, and a semver prerelease too.
        gh.release("foodstuffs-nz-cli/v9.9.1-rc.1", &assets),
    ]))
    .await;

    let out = f
        .cmd()
        .env("FSNZ_UPDATE_API", gh.server.uri())
        .args(["--json", "update", "--check"])
        .output()
        .unwrap();

    // A pending update is a non-zero exit, so `fsnz update --check` can gate a
    // script the way `doctor` does.
    assert!(!out.status.success(), "an update is available");
    let json = stdout_json(&out);
    assert_eq!(json["current"], CURRENT);
    assert_eq!(json["latest"], NEWER);
    assert_eq!(json["tag"], format!("foodstuffs-nz-cli/v{NEWER}"));
    assert_eq!(json["update_available"], true);
    assert_eq!(json["installed"], false);
    assert_eq!(json["asset"], assets[0]);
}

#[tokio::test]
async fn check_is_happy_when_the_newest_release_is_the_one_running() {
    let f = Fixture::start().await;
    let gh = Github::start().await;
    gh.releases(json!([
        gh.release(&format!("foodstuffs-nz-cli/v{CURRENT}"), &[]),
        gh.release("foodstuffs-nz-cli/v0.0.1", &[]),
        gh.release("other-project/v42.0.0", &[]),
    ]))
    .await;

    let out = f
        .cmd()
        .env("FSNZ_UPDATE_API", gh.server.uri())
        .args(["update", "--check"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("is the latest release"), "got: {stdout}");
}

#[tokio::test]
async fn a_project_with_no_releases_is_not_an_error() {
    let f = Fixture::start().await;
    let gh = Github::start().await;
    gh.releases(json!([gh.release("other-project/v42.0.0", &[])]))
        .await;

    let out = f
        .cmd()
        .env("FSNZ_UPDATE_API", gh.server.uri())
        .args(["--json", "update", "--check"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout_json(&out)["latest"], serde_json::Value::Null);
    assert_eq!(stdout_json(&out)["update_available"], false);
}

#[cfg(unix)]
#[tokio::test]
async fn update_replaces_the_running_binary_and_records_where_it_came_from() {
    let f = Fixture::start().await;
    let gh = Github::start().await;
    // Executable and easy to identify: after the swap this is what runs.
    let payload = b"#!/bin/sh\necho replaced\n";
    let assets = mount_newer_release(&gh, payload).await;

    let dir = tempfile::tempdir().unwrap();
    let exe = binary_copy(dir.path());

    let out = f
        .cmd_at(&exe)
        .env("FSNZ_UPDATE_API", gh.server.uri())
        .arg("update")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("updating fsnz {CURRENT} -> {NEWER}")),
        "got: {stdout}"
    );
    assert!(stdout.contains("checksum verified"), "got: {stdout}");
    assert!(
        stdout.contains(&format!("installed fsnz {NEWER}")),
        "got: {stdout}"
    );

    // The file on disk is the one out of the tarball, and still executable.
    assert_eq!(std::fs::read(&exe).unwrap(), payload);
    let ran = std::process::Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "replaced");

    // Nothing is left staged beside it.
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n != "fsnz")
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");

    let marker: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(f.home.path().join("state/install.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(marker["version"], NEWER);
    assert_eq!(marker["tag"], format!("foodstuffs-nz-cli/v{NEWER}"));
    assert_eq!(marker["asset"], assets[0]);
    assert_eq!(
        marker["path"].as_str().unwrap(),
        std::fs::canonicalize(&exe).unwrap().to_str().unwrap()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_download_that_does_not_match_its_checksum_is_not_installed() {
    let f = Fixture::start().await;
    let gh = Github::start().await;
    let names = host_asset_names(NEWER);
    let archive = tarball("fsnz", b"#!/bin/sh\necho replaced\n");

    for name in &names {
        gh.asset(name, archive.clone()).await;
    }
    // Checksums for something else entirely, as a tampered mirror would serve.
    let decoys: Vec<(String, Vec<u8>)> = names
        .iter()
        .map(|n| (n.clone(), b"not what was downloaded".to_vec()))
        .collect();
    gh.asset("SHA256SUMS", sha256sums(&decoys)).await;

    let mut assets = names.clone();
    assets.push("SHA256SUMS".to_string());
    gh.releases(json!([
        gh.release(&format!("foodstuffs-nz-cli/v{NEWER}"), &assets)
    ]))
    .await;

    let dir = tempfile::tempdir().unwrap();
    let exe = binary_copy(dir.path());
    let before = std::fs::read(&exe).unwrap();

    let out = f
        .cmd_at(&exe)
        .env("FSNZ_UPDATE_API", gh.server.uri())
        .arg("update")
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("refusing to install"), "got: {stderr}");
    assert_eq!(
        std::fs::read(&exe).unwrap(),
        before,
        "the binary was replaced anyway"
    );
    assert!(!f.home.path().join("state/install.json").exists());
}

#[tokio::test]
async fn a_release_with_nothing_built_for_this_host_says_what_it_does_have() {
    let f = Fixture::start().await;
    let gh = Github::start().await;
    gh.releases(json!([gh.release(
        &format!("foodstuffs-nz-cli/v{NEWER}"),
        &[
            format!("foodstuffs-nz-cli-{NEWER}-aix-ppc64.tar.gz"),
            "SHA256SUMS".to_string(),
        ],
    )]))
    .await;

    let out = f
        .cmd()
        .env("FSNZ_UPDATE_API", gh.server.uri())
        .arg("update")
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("aix-ppc64"), "got: {stderr}");
    assert!(stderr.contains("cargo install"), "got: {stderr}");
    // SHA256SUMS is not a platform and has no business in that list.
    assert!(!stderr.contains("SHA256SUMS"), "got: {stderr}");
}

#[tokio::test]
async fn being_rate_limited_says_so_rather_than_reporting_no_releases() {
    let f = Fixture::start().await;
    let gh = Github::start().await;
    gh.releases_status(403, r#"{"message":"API rate limit exceeded"}"#)
        .await;

    let out = f
        .cmd()
        .env("FSNZ_UPDATE_API", gh.server.uri())
        .args(["update", "--check"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("rate limit"), "got: {stderr}");
    assert!(stderr.contains("GITHUB_TOKEN"), "got: {stderr}");
}

#[cfg(unix)]
#[tokio::test]
async fn the_version_says_when_update_is_what_put_the_binary_there() {
    let f = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let exe = binary_copy(dir.path());

    let state = f.home.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        state.join("install.json"),
        json!({
            "version": NEWER,
            "tag": format!("foodstuffs-nz-cli/v{NEWER}"),
            "url": "https://example.invalid/release",
            "asset": host_asset_names(NEWER)[0],
            "path": std::fs::canonicalize(&exe).unwrap(),
            "installed_at": 1_756_512_000u64,
        })
        .to_string(),
    )
    .unwrap();

    let out = f.cmd_at(&exe).arg("--version").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&format!("fsnz {CURRENT}")), "got: {stdout}");
    assert!(stdout.contains("binary"), "got: {stdout}");
    assert!(
        stdout.contains(&format!(
            "by `fsnz update` from foodstuffs-nz-cli/v{NEWER} on 2025-08-30"
        )),
        "got: {stdout}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_marker_left_by_a_different_copy_is_not_claimed_as_this_ones() {
    let f = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let exe = binary_copy(dir.path());

    let state = f.home.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        state.join("install.json"),
        json!({
            "version": NEWER,
            "tag": format!("foodstuffs-nz-cli/v{NEWER}"),
            "url": "https://example.invalid/release",
            "asset": "irrelevant.tar.gz",
            "path": "/somewhere/else/fsnz",
            "installed_at": 1_756_512_000u64,
        })
        .to_string(),
    )
    .unwrap();

    let out = f.cmd_at(&exe).arg("--version").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("fsnz update"), "got: {stdout}");
}
