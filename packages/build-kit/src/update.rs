//! Finding and installing newer releases.
//!
//! Releases live in a monorepo, one tag namespace per project
//! (`<project>/vX.Y.Z`), and every project in it releases on its own schedule.
//! That rules out GitHub's `releases/latest`, which answers with the newest
//! release of *anything* in the repository -- usually somebody else's project.
//! So this lists releases and picks the newest tag in one namespace itself.

use std::io::Read;
use std::path::{Path, PathBuf};

use net_kit::wreq;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

pub const GITHUB_API: &str = "https://api.github.com";

/// Where to look, and as whom.
///
/// A plain struct the caller fills: this crate does not read the environment,
/// so `<PREFIX>_UPDATE_API` and `GITHUB_TOKEN` are the app's to resolve.
#[derive(Clone, Debug)]
pub struct Source {
    pub repo: String,
    /// The tag namespace, which is the project's directory in the monorepo.
    /// Deliberately separate from the tool's config-directory name: it is a
    /// property of the repository layout, not of the tool.
    pub project: String,
    pub api_base: String,
    pub token: Option<String>,
    /// Sent as `User-Agent`, which is what GitHub asks for and what shows up in
    /// their rate-limit accounting.
    pub user_agent: String,
}

impl Source {
    pub fn new(repo: impl Into<String>, project: impl Into<String>, version: &str) -> Source {
        let project = project.into();
        Source {
            user_agent: format!("{project}/{version}"),
            repo: repo.into(),
            project,
            api_base: GITHUB_API.to_string(),
            token: None,
        }
    }

    pub fn with_api_base(mut self, base: impl Into<String>) -> Source {
        self.api_base = base.into().trim_end_matches('/').to_string();
        self
    }

    pub fn with_token(mut self, token: Option<String>) -> Source {
        self.token = token.filter(|t| !t.trim().is_empty());
        self
    }

    /// Whether the token may be sent.
    ///
    /// Only ever to GitHub itself: the API base can be pointed anywhere, and a
    /// token is not something to hand to whatever it points at.
    fn authorised(&self) -> bool {
        self.api_base == GITHUB_API
    }

    /// This project's tags only. Every other project's tags in the monorepo
    /// fail the prefix and are dropped.
    fn version_from_tag(&self, tag: &str) -> Option<Version> {
        Version::parse(tag.strip_prefix(&format!("{}/v", self.project))?).ok()
    }
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    pub url: String,
    pub assets: Vec<Asset>,
    pub prerelease: bool,
    /// The release notes, as markdown. Empty when the release has none.
    pub notes: String,
}

#[derive(Clone, Debug)]
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
    body: String,
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

    pub fn checksums(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == "SHA256SUMS")
    }

    /// The platforms this release does have artifacts for, so an error can say
    /// what *is* on offer.
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

/// How a release names this host: `uname -s` lowercased, then `uname -m`.
///
/// Both halves have more than one spelling in the wild -- `uname -m` is `arm64`
/// on macOS and `aarch64` on Linux for the same chip -- so this returns every
/// name the host could plausibly be published under, best first.
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

/// Every published release of one project, newest first.
///
/// Prereleases are included; which of them a given binary may move to is
/// [`pick`]'s decision, not this one's.
pub async fn releases(http: &wreq::Client, src: &Source) -> Result<Vec<Release>> {
    // One page. A hundred releases back is far past the point where the newest
    // one would have fallen off the end.
    let url = format!("{}/repos/{}/releases?per_page=100", src.api_base, src.repo);
    let mut req = http
        .get(&url)
        .header(wreq::header::USER_AGENT, src.user_agent.clone())
        .header(wreq::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if src.authorised() {
        if let Some(token) = &src.token {
            req = req.bearer_auth(token);
        }
    }

    let wire: Vec<WireRelease> = match net_kit::http::json("GET", &url, req.send().await).await {
        Ok(wire) => wire,
        Err(e) => {
            if matches!(e.status(), Some(403) | Some(429)) {
                return Err(Error::RateLimited {
                    status: e.status().unwrap_or(403),
                });
            }
            return Err(e.into());
        }
    };

    let mut out: Vec<Release> = wire
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = src.version_from_tag(&r.tag_name)?;
            Some(Release {
                // A tag carrying a semver prerelease is a prerelease whatever
                // the release was flagged as.
                prerelease: r.prerelease || !version.pre.is_empty(),
                version,
                tag: r.tag_name,
                url: r.html_url,
                notes: notes(&r.body),
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

/// GitHub returns a release body with CRLF endings and usually a trailing
/// newline; a terminal wants neither.
fn notes(body: &str) -> String {
    body.replace("\r\n", "\n").trim().to_string()
}

/// The release to move to, or `None` when there is none.
///
/// A stable build only ever sees stable releases. A prerelease build is on its
/// way back to the stable channel: it takes a newer stable when one exists, and
/// otherwise carries on through the previews. `pre` opts in to previews from
/// either channel, which is what `--pre-release` asks for.
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

/// The newest preview ahead of `current`, for `--check` to mention.
pub fn newest_preview<'a>(releases: &'a [Release], current: &Version) -> Option<&'a Release> {
    releases
        .iter()
        .find(|r| r.prerelease && r.version > *current)
}

/// Every release being crossed to reach `to`, newest first -- the changelog
/// `--check` prints.
///
/// Previews are left out unless `to` is itself one: a stable build stepping
/// over 1.1.0-rc.1 on its way to 1.1.0 was never offered that release, so its
/// notes are not part of what changed.
pub fn changelog<'a>(releases: &'a [Release], from: &Version, to: &'a Release) -> Vec<&'a Release> {
    // An explicit downgrade crosses nothing. The notes of the version asked
    // about are still the answer to what it is.
    if to.version <= *from {
        return vec![to];
    }
    releases
        .iter()
        .filter(|r| r.version > *from && r.version <= to.version)
        .filter(|r| !r.prerelease || to.prerelease)
        .collect()
}

/// A release by exact version, for an explicit `update <version>` -- including
/// a downgrade.
pub fn find<'a>(releases: &'a [Release], want: &Version) -> Option<&'a Release> {
    releases.iter().find(|r| r.version == *want)
}

/// `0.1.4-rc.2` or `v0.1.4-rc.2`; the `v` matches the tag and is optional.
pub fn parse_version(text: &str) -> Result<Version> {
    let text = text.trim();
    let bare = text.strip_prefix('v').unwrap_or(text);
    Version::parse(bare).map_err(|_| Error::BadVersion(text.to_string()))
}

/// Download `asset`, check it against the release's `SHA256SUMS`, and replace
/// `target` with what is inside.
///
/// `target` is a parameter rather than `current_exe()` read internally, which
/// is what makes staging, verification, extraction and the swap testable over a
/// temp directory instead of requiring a second binary to be built and run.
///
/// `report` is called with each step, so the caller decides whether progress is
/// printed -- under `--json` it is not.
pub async fn install(
    http: &wreq::Client,
    src: &Source,
    release: &Release,
    asset: &Asset,
    target: &Path,
    report: &dyn Fn(&str),
) -> Result<PathBuf> {
    let dir = target
        .parent()
        .ok_or_else(|| Error::Archive(format!("{} has no parent directory", target.display())))?;

    // Stage the replacement beside the binary it replaces: the swap at the end
    // is a rename, which only works within one filesystem. Doing it before the
    // download means an unwritable install directory fails in a second rather
    // than after several megabytes.
    let staged = Staged::create(dir, target)?;

    report(&format!("downloading {}", asset.name));
    let tarball = fetch(http, src, &asset.url).await?;

    let sums = release.checksums().ok_or_else(|| Error::Unverifiable {
        tag: release.tag.clone(),
        url: release.url.clone(),
    })?;
    let sums = fetch(http, src, &sums.url).await?;
    verify(&tarball, &String::from_utf8_lossy(&sums), &asset.name)?;
    report("checksum verified");

    let binary = extract(&tarball, target.file_name().and_then(|n| n.to_str()))?;
    staged.write(&binary)?;
    staged.commit(target)?;

    Ok(target.to_path_buf())
}

/// A release asset URL is a 302 to release-assets.githubusercontent.com. The
/// policy is set per request so this does not depend on the shared client's --
/// one of the two vendors deliberately builds a client that follows none.
async fn fetch(http: &wreq::Client, src: &Source, url: &str) -> Result<Vec<u8>> {
    let res = http
        .get(url)
        .header(wreq::header::USER_AGENT, src.user_agent.clone())
        .redirect(wreq::redirect::Policy::limited(10))
        .send()
        .await
        .map_err(|source| net_kit::HttpError::Transport {
            method: "GET",
            url: url.to_string(),
            source,
        })?;
    let status = res.status();
    if !status.is_success() {
        return Err(net_kit::HttpError::Status {
            method: "GET",
            url: url.to_string(),
            status: status.as_u16(),
            detail: String::new(),
            body: String::new(),
        }
        .into());
    }
    Ok(res
        .bytes()
        .await
        .map_err(|source| net_kit::HttpError::Transport {
            method: "GET",
            url: url.to_string(),
            source,
        })?
        .into())
}

/// Check `bytes` against the line for `name` in a `sha256sum`-format file.
pub fn verify(bytes: &[u8], sums: &str, name: &str) -> Result<()> {
    let expected = sums
        .lines()
        .filter_map(|line| line.split_once("  ").or_else(|| line.split_once(' ')))
        // The name column may be marked binary with a leading `*`.
        .find(|(_, file)| file.trim().trim_start_matches('*') == name)
        .map(|(hash, _)| hash.trim().to_lowercase())
        .ok_or_else(|| Error::Archive(format!("SHA256SUMS has no line for {name}")))?;

    let actual = hex(&Sha256::digest(bytes));
    if actual != expected {
        return Err(Error::ChecksumMismatch {
            name: name.to_string(),
            expected,
            actual,
        });
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Pull the executable out of a `.tar.gz`.
///
/// The release archives the binary alone at the root, so the first regular file
/// matching the name is it. Anything else in the archive is ignored.
pub fn extract(tarball: &[u8], expected: Option<&str>) -> Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(tarball));
    let mut archive = tar::Archive::new(decoder);
    let mut fallback = None;

    let entries = archive
        .entries()
        .map_err(|e| Error::Archive(format!("reading the archive: {e}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| Error::Archive(format!("reading an entry: {e}")))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|e| Error::Archive(format!("reading an entry path: {e}")))?
            .into_owned();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| Error::Archive(format!("reading {name}: {e}")))?;
        if Some(name.as_str()) == expected {
            return Ok(bytes);
        }
        fallback.get_or_insert(bytes);
    }

    // A tarball with one file under some other name is still usable; an empty
    // one is not.
    fallback.ok_or_else(|| Error::Archive("the archive contains no files".into()))
}

/// The replacement binary, written beside the one it replaces and cleaned up if
/// anything goes wrong before the swap.
struct Staged {
    path: PathBuf,
    mode: u32,
    committed: bool,
}

impl Staged {
    fn create(dir: &Path, target: &Path) -> Result<Staged> {
        let name = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("tool");
        let path = dir.join(format!(".{name}.new-{}", std::process::id()));
        std::fs::File::create(&path).map_err(|source| Error::NotWritable {
            dir: dir.display().to_string(),
            source,
        })?;
        Ok(Staged {
            // Match whatever the current binary is set to, so an install into a
            // shared location keeps the group and other bits it was given.
            mode: mode_of(target).unwrap_or(0o755),
            path,
            committed: false,
        })
    }

    fn write(&self, bytes: &[u8]) -> Result<()> {
        std::fs::write(&self.path, bytes)
            .map_err(|e| Error::io(format!("writing {}", self.path.display()), e))?;
        set_mode(&self.path, self.mode)
            .map_err(|e| Error::io(format!("making {} executable", self.path.display()), e))
    }

    /// Rename over the target. On unix the running process keeps the file it
    /// already opened, so replacing a binary under itself is safe.
    fn commit(mut self, target: &Path) -> Result<()> {
        std::fs::rename(&self.path, target)
            .map_err(|e| Error::io(format!("replacing {}", target.display()), e))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Source {
        Source::new("owner/repo", "grocery-nz-cli", "1.0.0")
    }

    fn release(tag: &str, prerelease: bool, assets: &[&str]) -> Release {
        let src = source();
        Release {
            version: src.version_from_tag(tag).expect("a tag in the namespace"),
            tag: tag.into(),
            url: format!("https://example.test/{tag}"),
            prerelease,
            notes: format!("what changed in {tag}"),
            assets: assets
                .iter()
                .map(|name| Asset {
                    name: (*name).into(),
                    url: format!("https://example.test/assets/{name}"),
                })
                .collect(),
        }
    }

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn only_this_projects_tags_are_releases() {
        let src = source();
        assert_eq!(
            src.version_from_tag("grocery-nz-cli/v1.2.3"),
            Some(v("1.2.3"))
        );
        // The neighbour's release, sitting on the same commits.
        assert_eq!(src.version_from_tag("woolworths-nz-cli/v9.9.9"), None);
        assert_eq!(src.version_from_tag("v1.2.3"), None);
        assert_eq!(src.version_from_tag("grocery-nz-cli/v-nonsense"), None);
    }

    #[test]
    fn a_stable_build_never_sees_a_preview() {
        let releases = vec![release("grocery-nz-cli/v2.0.0-rc.1", true, &[])];
        assert!(pick(&releases, &v("1.0.0"), false).is_none());
        assert_eq!(
            pick(&releases, &v("1.0.0"), true).map(|r| r.tag.as_str()),
            Some("grocery-nz-cli/v2.0.0-rc.1"),
            "--pre-release opts in"
        );
    }

    #[test]
    fn a_preview_build_takes_a_newer_stable_when_one_exists() {
        let releases = vec![
            release("grocery-nz-cli/v2.0.0-rc.2", true, &[]),
            release("grocery-nz-cli/v1.9.0", false, &[]),
        ];
        // On 2.0.0-rc.1: the way back to the stable channel is 1.9.0? No --
        // it is not newer. Only rc.2 is.
        assert_eq!(
            pick(&releases, &v("2.0.0-rc.1"), false).map(|r| r.tag.as_str()),
            Some("grocery-nz-cli/v2.0.0-rc.2"),
            "no newer stable, so carry on through the previews"
        );
        // On 1.8.0, the stable 1.9.0 wins over the newer preview.
        assert_eq!(
            pick(&releases, &v("1.8.0"), false).map(|r| r.tag.as_str()),
            Some("grocery-nz-cli/v1.9.0")
        );
    }

    #[test]
    fn nothing_newer_is_nothing_to_do() {
        let releases = vec![release("grocery-nz-cli/v1.0.0", false, &[])];
        assert!(pick(&releases, &v("1.0.0"), false).is_none());
        assert!(pick(&releases, &v("2.0.0"), true).is_none());
    }

    #[test]
    fn an_explicit_version_allows_a_downgrade() {
        let releases = vec![
            release("grocery-nz-cli/v2.0.0", false, &[]),
            release("grocery-nz-cli/v1.0.0", false, &[]),
        ];
        assert_eq!(
            find(&releases, &v("1.0.0")).map(|r| r.tag.as_str()),
            Some("grocery-nz-cli/v1.0.0")
        );
        assert!(find(&releases, &v("3.0.0")).is_none());
    }

    #[test]
    fn the_changelog_is_every_release_being_crossed() {
        let releases = vec![
            release("grocery-nz-cli/v1.3.0", false, &[]),
            release("grocery-nz-cli/v1.2.0", false, &[]),
            release("grocery-nz-cli/v1.1.0", false, &[]),
            release("grocery-nz-cli/v1.0.0", false, &[]),
        ];
        let to = &releases[0];
        let tags: Vec<&str> = changelog(&releases, &v("1.1.0"), to)
            .iter()
            .map(|r| r.tag.as_str())
            .collect();
        assert_eq!(
            tags,
            ["grocery-nz-cli/v1.3.0", "grocery-nz-cli/v1.2.0"],
            "newest first, and the version already installed is not news"
        );
    }

    #[test]
    fn the_changelog_skips_previews_a_stable_build_was_never_offered() {
        let releases = vec![
            release("grocery-nz-cli/v1.1.0", false, &[]),
            release("grocery-nz-cli/v1.1.0-rc.1", true, &[]),
        ];
        assert_eq!(changelog(&releases, &v("1.0.0"), &releases[0]).len(), 1);
        // ...but a run heading for the preview itself sees it.
        assert_eq!(changelog(&releases, &v("1.0.0"), &releases[1]).len(), 1);
    }

    #[test]
    fn a_downgrade_still_says_what_the_version_asked_about_is() {
        let releases = vec![
            release("grocery-nz-cli/v2.0.0", false, &[]),
            release("grocery-nz-cli/v1.0.0", false, &[]),
        ];
        let tags: Vec<&str> = changelog(&releases, &v("2.0.0"), &releases[1])
            .iter()
            .map(|r| r.tag.as_str())
            .collect();
        assert_eq!(tags, ["grocery-nz-cli/v1.0.0"]);
    }

    #[test]
    fn release_notes_arrive_with_crlf_endings() {
        assert_eq!(
            notes("## Features\r\n\r\n* a thing\r\n"),
            "## Features\n\n* a thing"
        );
        assert_eq!(notes(""), "");
    }

    #[test]
    fn a_tag_with_a_semver_prerelease_is_a_preview_however_it_was_flagged() {
        // GitHub's `prerelease` checkbox is not the authority here.
        let r = release("grocery-nz-cli/v2.0.0-rc.1", false, &[]);
        let releases = vec![Release {
            prerelease: r.prerelease || !r.version.pre.is_empty(),
            ..r
        }];
        assert!(pick(&releases, &v("1.0.0"), false).is_none());
    }

    #[test]
    fn version_parsing_takes_the_tags_v_or_not() {
        assert_eq!(parse_version("1.2.3").unwrap(), v("1.2.3"));
        assert_eq!(parse_version("v1.2.3").unwrap(), v("1.2.3"));
        assert_eq!(parse_version(" v1.2.3 ").unwrap(), v("1.2.3"));
        assert!(parse_version("latest").is_err());
    }

    #[test]
    fn host_platforms_offers_every_spelling_best_first() {
        let names = host_platforms();
        assert!(!names.is_empty());
        if std::env::consts::ARCH == "aarch64" {
            let arm = names.iter().position(|n| n.ends_with("-arm64"));
            let aarch = names.iter().position(|n| n.ends_with("-aarch64"));
            assert!(arm < aarch, "arm64 is the common spelling: {names:?}");
        }
        if std::env::consts::OS == "macos" {
            assert!(names.iter().all(|n| n.starts_with("darwin-")), "{names:?}");
        }
    }

    #[test]
    fn an_asset_is_matched_by_platform_suffix() {
        let host = host_platforms().first().cloned().unwrap();
        let r = release(
            "grocery-nz-cli/v1.0.0",
            false,
            &[
                "grocery-nz-cli-1.0.0-plan9-vax.tar.gz",
                &format!("grocery-nz-cli-1.0.0-{host}.tar.gz"),
                "SHA256SUMS",
            ],
        );
        assert_eq!(
            r.asset_for_host().map(|a| a.name.as_str()),
            Some(format!("grocery-nz-cli-1.0.0-{host}.tar.gz").as_str())
        );
        assert!(r.checksums().is_some());
        assert!(r.platforms().contains(&"plan9-vax".to_string()));
    }

    #[test]
    fn a_release_without_this_host_offers_nothing() {
        let r = release(
            "grocery-nz-cli/v1.0.0",
            false,
            &["grocery-nz-cli-1.0.0-plan9-vax.tar.gz"],
        );
        assert!(r.asset_for_host().is_none());
        // and the error can say what there *is*
        assert_eq!(r.platforms(), vec!["plan9-vax".to_string()]);
    }

    #[test]
    fn a_release_with_no_checksums_cannot_be_verified() {
        let r = release("grocery-nz-cli/v1.0.0", false, &["binary.tar.gz"]);
        assert!(r.checksums().is_none());
    }

    fn sha256sums(name: &str, bytes: &[u8]) -> String {
        format!("{}  {name}\n", hex(&Sha256::digest(bytes)))
    }

    #[test]
    fn verify_accepts_the_published_checksum() {
        let bytes = b"a tarball";
        assert!(verify(bytes, &sha256sums("tool.tar.gz", bytes), "tool.tar.gz").is_ok());
    }

    #[test]
    fn verify_accepts_the_binary_marker_and_single_space_forms() {
        let bytes = b"a tarball";
        let digest = hex(&Sha256::digest(bytes));
        assert!(verify(bytes, &format!("{digest} *tool.tar.gz\n"), "tool.tar.gz").is_ok());
    }

    #[test]
    fn verify_refuses_a_mismatch_and_says_both_hashes() {
        let err = verify(
            b"different",
            &sha256sums("tool.tar.gz", b"original"),
            "tool.tar.gz",
        )
        .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("refusing to install"), "{text}");
        assert!(text.contains("expected"), "{text}");
    }

    #[test]
    fn verify_refuses_when_the_file_is_not_listed() {
        let err = verify(b"x", &sha256sums("other.tar.gz", b"x"), "tool.tar.gz").unwrap_err();
        assert!(err.to_string().contains("no line for tool.tar.gz"), "{err}");
    }

    fn tarball(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn extract_prefers_the_entry_named_like_the_target() {
        let archive = tarball(&[("README", b"docs"), ("gsnz", b"the binary")]);
        assert_eq!(extract(&archive, Some("gsnz")).unwrap(), b"the binary");
    }

    #[test]
    fn extract_falls_back_to_the_only_file_when_the_name_differs() {
        let archive = tarball(&[("gsnz-renamed", b"the binary")]);
        assert_eq!(extract(&archive, Some("gsnz")).unwrap(), b"the binary");
    }

    #[test]
    fn extract_refuses_an_empty_archive() {
        let err = extract(&tarball(&[]), Some("gsnz")).unwrap_err();
        assert!(err.to_string().contains("no files"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn staging_preserves_the_targets_mode_and_replaces_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let target = dir.path().join("gsnz");
        std::fs::write(&target, b"old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o750)).unwrap();

        let staged = Staged::create(dir.path(), &target).unwrap();
        staged.write(b"new").unwrap();
        staged.commit(&target).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o750, "the mode the file already had is kept");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no staging file left behind"
        );
    }

    #[test]
    fn an_uncommitted_staging_file_is_cleaned_up() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = dir.path().join("gsnz");
        std::fs::write(&target, b"old").unwrap();
        {
            let staged = Staged::create(dir.path(), &target).unwrap();
            staged.write(b"new").unwrap();
            // dropped without commit, as any error before the swap would
        }
        assert_eq!(std::fs::read(&target).unwrap(), b"old", "target untouched");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn staging_fails_fast_when_the_directory_is_not_writable() {
        let target = Path::new("/nonexistent-directory-xyz/gsnz");
        let Err(err) = Staged::create(target.parent().unwrap(), target) else {
            panic!("staging should not succeed in a directory that does not exist");
        };
        assert!(err.to_string().contains("needs write access"), "{err}");
    }
}
