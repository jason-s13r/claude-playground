//! Finding and installing newer releases of this tool.
//!
//! Releases live in a monorepo, one tag namespace per project
//! (`foodstuffs-nz-cli/vX.Y.Z`), and every project in it releases on its own
//! schedule. That rules out GitHub's `releases/latest`, which answers with the
//! newest release of *anything* in the repository -- usually somebody else's
//! project. So this lists releases and picks the newest tag in this project's
//! namespace itself.

use anyhow::{bail, Context, Result};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::build::{self, Install};
use crate::config::Paths;

/// Where the releases are published.
const REPO: &str = "jason-s13r/claude-playground";

/// The tag namespace, which is the project's directory name in the monorepo.
/// Deliberately its own constant: it is a property of the repository layout,
/// not of the config directory that happens to share the name.
const PROJECT: &str = "foodstuffs-nz-cli";

const DEFAULT_API: &str = "https://api.github.com";

/// The tool's own name and version, which is what GitHub's API asks for and
/// what shows up in their rate-limit accounting.
fn user_agent() -> String {
    format!("{PROJECT}/{}", build::VERSION)
}

/// Overridable so the tests can point the whole flow at a local server.
fn api_base() -> String {
    std::env::var("FSNZ_UPDATE_API")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_API.to_string())
        .trim_end_matches('/')
        .to_string()
}

pub struct Release {
    pub version: Version,
    pub tag: String,
    pub url: String,
    pub assets: Vec<Asset>,
    pub prerelease: bool,
}

pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Deserialize)]
struct WireRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<WireAsset>,
}

#[derive(Deserialize)]
struct WireAsset {
    name: String,
    browser_download_url: String,
}

impl Release {
    /// The artifact for the machine this is running on, if the release carries
    /// one. Releases are built per platform, and a project may not publish for
    /// every platform it can be compiled on.
    pub fn asset_for_host(&self) -> Option<&Asset> {
        host_platforms().iter().find_map(|platform| {
            let suffix = format!("-{platform}.tar.gz");
            self.assets.iter().find(|a| a.name.ends_with(&suffix))
        })
    }

    /// The platforms this release does have artifacts for, for an error message
    /// that says what *is* on offer.
    pub fn platforms(&self) -> Vec<String> {
        self.assets
            .iter()
            .filter_map(|a| {
                let stem = a.name.strip_suffix(".tar.gz")?;
                // `<project>-<version>-<os>-<arch>`: the platform is the tail.
                let mut parts = stem.rsplitn(3, '-');
                let arch = parts.next()?;
                let os = parts.next()?;
                Some(format!("{os}-{arch}"))
            })
            .collect()
    }
}

/// How `make dist` names the platform: `uname -s` lowercased, then `uname -m`.
///
/// Both halves have more than one spelling in the wild -- `uname -m` is `arm64`
/// on macOS and `aarch64` on Linux for the same architecture -- so this returns
/// every name the host could plausibly be published under, best first.
pub fn host_platforms() -> Vec<String> {
    let os: Vec<&str> = match std::env::consts::OS {
        "macos" => vec!["darwin"],
        other => vec![other],
    };
    let arch: Vec<&str> = match std::env::consts::ARCH {
        "aarch64" => vec!["arm64", "aarch64"],
        "x86_64" => vec!["x86_64", "amd64"],
        other => vec![other],
    };
    os.iter()
        .flat_map(|o| arch.iter().map(move |a| format!("{o}-{a}")))
        .collect()
}

/// Every published release of this project, newest first.
///
/// Prereleases are included; which of them a given binary may move to is
/// [`pick`]'s decision, not this one's.
pub async fn releases(http: &reqwest::Client) -> Result<Vec<Release>> {
    // One page. A hundred releases back is far past the point where the newest
    // one would have fallen off the end.
    let url = format!("{}/repos/{REPO}/releases?per_page=100", api_base());
    let mut req = http
        .get(&url)
        .header(reqwest::header::USER_AGENT, user_agent())
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    // Only ever to GitHub itself. FSNZ_UPDATE_API can point anywhere, and a
    // token is not something to hand to whatever it points at.
    if api_base() == DEFAULT_API {
        if let Some(token) = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GH_TOKEN"))
            .ok()
            .filter(|t| !t.trim().is_empty())
        {
            req = req.bearer_auth(token);
        }
    }

    let res = req.send().await.with_context(|| format!("GET {url}"))?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            bail!(
                "GitHub declined the update check ({status}). This is usually the \
                 unauthenticated rate limit; set GITHUB_TOKEN, or try again later."
            );
        }
        bail!("GET {url} returned {status}: {}", excerpt(&body));
    }

    let wire: Vec<WireRelease> = res.json().await.context("reading the release list")?;
    let mut out: Vec<Release> = wire
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = version_from_tag(&r.tag_name)?;
            Some(Release {
                // A tag carrying a semver prerelease is a prerelease whatever
                // the release was flagged as.
                prerelease: r.prerelease || !version.pre.is_empty(),
                version,
                tag: r.tag_name,
                url: r.html_url,
                assets: r
                    .assets
                    .into_iter()
                    .map(|a| Asset {
                        name: a.name,
                        url: a.browser_download_url,
                    })
                    .collect(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(out)
}

/// The release `fsnz update` should move to, or `None` when there is none.
///
/// A stable build only ever sees stable releases. A prerelease build is on its
/// way back to the stable channel: it takes a newer stable when one exists,
/// and otherwise carries on through the previews. `pre` opts in to previews
/// from either channel, which is what `--pre-release` asks for.
pub fn pick<'a>(releases: &'a [Release], current: &Version, pre: bool) -> Option<&'a Release> {
    let newer = || releases.iter().filter(|r| r.version > *current);
    if pre {
        return newer().next();
    }
    let stable = newer().find(|r| !r.prerelease);
    if stable.is_some() || current.pre.is_empty() {
        return stable;
    }
    newer().next()
}

/// The newest preview ahead of `current`, for `--check` to mention. `None`
/// when the newest thing available is already what [`pick`] would take.
pub fn newest_preview<'a>(releases: &'a [Release], current: &Version) -> Option<&'a Release> {
    releases
        .iter()
        .find(|r| r.prerelease && r.version > *current)
}

/// A release by exact version, for `fsnz update <version>`.
pub fn find<'a>(releases: &'a [Release], want: &Version) -> Option<&'a Release> {
    releases.iter().find(|r| r.version == *want)
}

/// `0.1.4-rc.2` or `v0.1.4-rc.2`; the `v` matches the tag and is optional.
pub fn parse_version(text: &str) -> Result<Version> {
    let text = text.trim();
    let bare = text.strip_prefix('v').unwrap_or(text);
    Version::parse(bare).with_context(|| format!("`{text}` is not a version"))
}

/// This project's tags only: `<project>/vX.Y.Z`. Every other project's tags in
/// the monorepo fail the prefix and are dropped.
fn version_from_tag(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix(&format!("{PROJECT}/v"))?).ok()
}

/// The version this binary reports.
pub fn current() -> Result<Version> {
    Version::parse(build::VERSION).context("parsing this build's own version")
}

/// Download `asset`, check it against the release's `SHA256SUMS`, and replace
/// the running binary with what is inside. Returns the path that was replaced.
///
/// `report` is called with each step as it happens, so the caller decides
/// whether progress is printed.
pub async fn install(
    http: &reqwest::Client,
    release: &Release,
    asset: &Asset,
    paths: &Paths,
    report: &dyn Fn(&str),
) -> Result<PathBuf> {
    let exe = build::exe_path().context(
        "could not work out which file this binary is; \
         download the release manually instead",
    )?;
    let dir = exe
        .parent()
        .with_context(|| format!("{} has no parent directory", exe.display()))?;

    // Stage the replacement beside the binary it replaces: the swap at the end
    // is a rename, which only works within one filesystem. Doing it before the
    // download means an unwritable install directory fails in a second rather
    // than after several megabytes.
    let staged = Staged::create(dir, &exe)?;

    report(&format!("downloading {}", asset.name));
    let tarball = fetch(http, &asset.url)
        .await
        .with_context(|| format!("downloading {}", asset.name))?;

    let sums = release
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .with_context(|| {
            format!(
                "{} publishes no SHA256SUMS, so the download cannot be verified; \
                 install it by hand from {} if you trust it",
                release.tag, release.url
            )
        })?;
    let sums = fetch(http, &sums.url)
        .await
        .context("downloading SHA256SUMS")?;
    verify(&tarball, &String::from_utf8_lossy(&sums), &asset.name)?;
    report("checksum verified");

    let binary = extract(&tarball, exe.file_name().and_then(|n| n.to_str()))
        .with_context(|| format!("unpacking {}", asset.name))?;

    staged.write(&binary)?;
    staged.commit(&exe)?;

    Install {
        version: release.version.to_string(),
        tag: release.tag.clone(),
        url: release.url.clone(),
        asset: asset.name.clone(),
        path: exe.clone(),
        installed_at: build::now(),
    }
    // A binary that landed correctly is not a failure just because the note
    // about it could not be filed.
    .save(paths)
    .unwrap_or_else(|e| report(&format!("note: could not record the install: {e:#}")));

    Ok(exe)
}

async fn fetch(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let res = http
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent())
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = res.status();
    if !status.is_success() {
        bail!("GET {url} returned {status}");
    }
    Ok(res
        .bytes()
        .await
        .context("reading the response body")?
        .into())
}

/// Check `bytes` against the line for `name` in a `sha256sum`-format file.
fn verify(bytes: &[u8], sums: &str, name: &str) -> Result<()> {
    let expected = sums
        .lines()
        .filter_map(|line| line.split_once("  ").or_else(|| line.split_once(' ')))
        // The name column may be marked binary with a leading `*`.
        .find(|(_, file)| file.trim().trim_start_matches('*') == name)
        .map(|(hash, _)| hash.trim().to_lowercase())
        .with_context(|| format!("SHA256SUMS has no line for {name}"))?;

    let actual = hex(&Sha256::digest(bytes));
    if actual != expected {
        bail!(
            "{name} does not match its published checksum\n  expected {expected}\n  \
             got      {actual}\nrefusing to install it"
        );
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Pull the executable out of a `.tar.gz`.
///
/// `make dist` archives the binary alone at the root of the tarball, so the
/// first regular file matching the name is it. `expected` is the running
/// binary's own file name; anything else in the archive is ignored.
fn extract(tarball: &[u8], expected: Option<&str>) -> Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(tarball));
    let mut archive = tar::Archive::new(decoder);
    let mut fallback = None;

    for entry in archive.entries().context("reading the archive")? {
        let mut entry = entry.context("reading an archive entry")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().context("reading an entry path")?.into_owned();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).context("reading an entry")?;
        if Some(name.as_str()) == expected || name == "fsnz" {
            return Ok(bytes);
        }
        fallback.get_or_insert(bytes);
    }

    // A tarball with one file in it under some other name is still usable; an
    // empty one is not.
    fallback.context("the archive contains no files")
}

/// The replacement binary, written beside the one it replaces and cleaned up if
/// anything goes wrong before the swap.
struct Staged {
    path: PathBuf,
    mode: u32,
    committed: bool,
}

impl Staged {
    fn create(dir: &Path, exe: &Path) -> Result<Staged> {
        let name = exe.file_name().and_then(|n| n.to_str()).unwrap_or("fsnz");
        let path = dir.join(format!(".{name}.new-{}", std::process::id()));
        std::fs::File::create(&path).map_err(|e| {
            anyhow::anyhow!(
                "cannot write to {}: {e}\nfsnz installs over itself, so it needs write \
                 access to the directory it lives in. Re-run with permission to write \
                 there, or unpack the release tarball by hand.",
                dir.display()
            )
        })?;
        Ok(Staged {
            // Match whatever the current binary is set to, so an install into a
            // shared location keeps the group and other bits it was given.
            mode: mode_of(exe).unwrap_or(0o755),
            path,
            committed: false,
        })
    }

    fn write(&self, bytes: &[u8]) -> Result<()> {
        std::fs::write(&self.path, bytes)
            .with_context(|| format!("writing {}", self.path.display()))?;
        set_mode(&self.path, self.mode)
            .with_context(|| format!("making {} executable", self.path.display()))
    }

    /// Rename over the running binary. On Unix the running process keeps the
    /// file it already opened, so replacing it under itself is safe.
    fn commit(mut self, exe: &Path) -> Result<()> {
        std::fs::rename(&self.path, exe).with_context(|| format!("replacing {}", exe.display()))?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn mode_of(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(std::fs::metadata(path).ok()?.permissions().mode())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn mode_of(_path: &Path) -> Option<u32> {
    None
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

fn excerpt(body: &str) -> String {
    let body = body.trim();
    match body.char_indices().nth(200) {
        Some((cut, _)) => format!("{}...", &body[..cut]),
        None => body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Newest first, as `releases` returns them.
    fn catalogue(versions: &[&str]) -> Vec<Release> {
        let mut out: Vec<Release> = versions
            .iter()
            .map(|v| {
                let version = Version::parse(v).unwrap();
                Release {
                    prerelease: !version.pre.is_empty(),
                    tag: format!("{PROJECT}/v{version}"),
                    url: String::new(),
                    assets: Vec::new(),
                    version,
                }
            })
            .collect();
        out.sort_by(|a, b| b.version.cmp(&a.version));
        out
    }

    fn picked(versions: &[&str], current: &str, pre: bool) -> Option<String> {
        let all = catalogue(versions);
        let current = Version::parse(current).unwrap();
        pick(&all, &current, pre).map(|r| r.version.to_string())
    }

    #[test]
    fn a_stable_build_is_never_offered_a_preview() {
        let all = ["0.1.3", "0.1.4-rc.1", "0.2.0-rc.1"];
        assert_eq!(picked(&all, "0.1.3", false), None);
        assert_eq!(
            picked(&["0.1.3", "0.1.4", "0.2.0-rc.1"], "0.1.3", false),
            Some("0.1.4".into())
        );
    }

    #[test]
    fn a_preview_build_takes_a_newer_stable_over_a_newer_preview() {
        // The way back to the stable channel: 0.1.4 wins even though
        // 0.1.5-rc.1 is higher.
        assert_eq!(
            picked(&["0.1.3", "0.1.4", "0.1.5-rc.1"], "0.1.4-rc.2", false),
            Some("0.1.4".into())
        );
    }

    #[test]
    fn a_preview_build_carries_on_through_previews_until_a_stable_appears() {
        // Nothing stable is ahead of rc.0, so the preview line continues.
        assert_eq!(
            picked(&["0.1.3", "0.1.4-rc.1", "0.1.4-rc.2"], "0.1.4-rc.0", false),
            Some("0.1.4-rc.2".into())
        );
        assert_eq!(picked(&["0.1.3", "0.1.4-rc.0"], "0.1.4-rc.0", false), None);
    }

    #[test]
    fn opting_in_takes_the_newest_of_either_channel() {
        assert_eq!(
            picked(&["0.1.3", "0.1.4-rc.1"], "0.1.3", true),
            Some("0.1.4-rc.1".into())
        );
        // A stable release that outranks every preview still wins.
        assert_eq!(
            picked(&["0.1.4", "0.1.4-rc.1"], "0.1.3", true),
            Some("0.1.4".into())
        );
    }

    #[test]
    fn a_version_is_accepted_with_or_without_its_leading_v() {
        assert_eq!(
            parse_version("0.1.4-rc.2").unwrap(),
            parse_version("v0.1.4-rc.2").unwrap()
        );
        assert!(parse_version("nonsense").is_err());
    }

    #[test]
    fn only_this_projects_tags_are_recognised() {
        assert_eq!(
            version_from_tag("foodstuffs-nz-cli/v1.2.3"),
            Some(Version::new(1, 2, 3))
        );
        assert!(version_from_tag("other-project/v1.2.3").is_none());
        assert!(version_from_tag("v1.2.3").is_none());
        // The namespace has to match in full, not just as a prefix.
        assert!(version_from_tag("foodstuffs-nz/v1.2.3").is_none());
        assert!(version_from_tag("foodstuffs-nz-cli/v1.2").is_none());
    }

    #[test]
    fn the_host_is_looked_up_under_every_name_uname_might_give_it() {
        let platforms = host_platforms();
        assert!(!platforms.is_empty());
        // uname disagrees with itself about aarch64 across the two platforms
        // this ships for, so both spellings have to be tried.
        if std::env::consts::ARCH == "aarch64" {
            assert!(
                platforms.iter().any(|p| p.ends_with("-arm64")),
                "{platforms:?}"
            );
            assert!(
                platforms.iter().any(|p| p.ends_with("-aarch64")),
                "{platforms:?}"
            );
        }
        if std::env::consts::OS == "macos" {
            assert!(
                platforms.iter().all(|p| p.starts_with("darwin-")),
                "{platforms:?}"
            );
        }
    }

    fn release_with(assets: &[&str]) -> Release {
        Release {
            version: Version::new(1, 0, 0),
            prerelease: false,
            tag: "foodstuffs-nz-cli/v1.0.0".into(),
            url: String::new(),
            assets: assets
                .iter()
                .map(|name| Asset {
                    name: name.to_string(),
                    url: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn an_asset_is_matched_by_the_platform_in_its_name() {
        let names: Vec<String> = host_platforms()
            .iter()
            .map(|p| format!("foodstuffs-nz-cli-1.0.0-{p}.tar.gz"))
            .collect();
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let release = release_with(&refs);
        assert_eq!(
            release.asset_for_host().map(|a| a.name.clone()),
            Some(refs[0].to_string())
        );

        let foreign = release_with(&["foodstuffs-nz-cli-1.0.0-aix-ppc64.tar.gz", "SHA256SUMS"]);
        assert!(foreign.asset_for_host().is_none());
        assert_eq!(foreign.platforms(), vec!["aix-ppc64".to_string()]);
    }

    #[test]
    fn a_download_that_does_not_match_its_checksum_is_refused() {
        let sums = format!("{}  payload.tar.gz\n", hex(&Sha256::digest(b"good")));
        assert!(verify(b"good", &sums, "payload.tar.gz").is_ok());

        let err = verify(b"tampered", &sums, "payload.tar.gz").unwrap_err();
        assert!(
            format!("{err:#}").contains("refusing to install"),
            "{err:#}"
        );

        let err = verify(b"good", &sums, "other.tar.gz").unwrap_err();
        assert!(
            format!("{err:#}").contains("no line for other.tar.gz"),
            "{err:#}"
        );
    }
}
