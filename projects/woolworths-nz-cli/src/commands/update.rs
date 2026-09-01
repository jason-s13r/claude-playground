//! `wwnz update` -- check for a newer release, and install it.

use anyhow::{bail, Result};

use crate::app::App;
use crate::build;
use crate::commands::io::print_json;
use crate::update::{self, Release};

/// Returns false when a newer release exists and was not installed, so
/// `wwnz update --check` can gate a script the way `doctor` does.
pub async fn run(app: &App, want: Option<&str>, check: bool, pre: bool) -> Result<bool> {
    let current = update::current()?;
    let releases = update::releases(&app.http).await?;

    let target = match want {
        Some(text) => {
            let version = update::parse_version(text)?;
            let Some(found) = update::find(&releases, &version) else {
                bail!("no release {version} was published; `wwnz update --check` says what is");
            };
            Some(found)
        }
        None => update::pick(&releases, &current, pre),
    };

    // Only worth saying when it is not already where this is heading.
    let preview = update::newest_preview(&releases, &current)
        .filter(|p| target.map(|t| t.version != p.version).unwrap_or(true));
    let preview_line = |p: Option<&Release>| {
        if let Some(p) = p {
            println!(
                "  preview {} available: `wwnz update --pre-release`",
                p.version
            );
        }
    };

    let Some(release) = target else {
        if app.json {
            print_json(&serde_json::json!({
                "current": current.to_string(),
                "latest": null,
                "preview": preview.map(|p| p.version.to_string()),
                "update_available": false,
                "installed": false,
            }));
        } else if releases.is_empty() {
            println!("{current} has no published releases yet");
        } else {
            println!("wwnz {current} is the latest release");
            preview_line(preview);
        }
        return Ok(true);
    };

    // An explicit version is a move to make even when it goes backwards.
    let available = release.version != current;
    let asset = release.asset_for_host();

    if check || !available {
        if app.json {
            print_json(&report(&current, release, available, false, preview));
        } else if available {
            let how = if release.version < current {
                " (downgrade)"
            } else {
                " available"
            };
            println!("wwnz {current} -> {}{how}", release.version);
            println!("  {}", release.url);
            let cmd = match want {
                Some(_) => format!("wwnz update {}", release.version),
                None if pre => "wwnz update --pre-release".to_string(),
                None => "wwnz update".to_string(),
            };
            match asset {
                Some(a) => println!("  run `{cmd}` to install {}", a.name),
                None => println!("  {}", no_asset_for_host(release)),
            }
            preview_line(preview);
        } else {
            println!("wwnz {current} is already installed");
            preview_line(preview);
        }
        return Ok(!available);
    }

    let Some(asset) = asset else {
        bail!(
            "wwnz {} is available, but {}\n  {}",
            release.version,
            no_asset_for_host(release),
            release.url
        );
    };

    // Nothing stops this from replacing a `cargo build` binary -- it is the
    // file on disk either way -- but it is worth saying out loud, because the
    // next `cargo build` will quietly put the local one back.
    if !app.json && build::PROFILE == "debug" {
        println!("note: replacing a locally built (debug) binary with a release build");
    }

    if !app.json {
        println!("updating wwnz {current} -> {}", release.version);
    }
    let report_step: &dyn Fn(&str) = if app.json {
        &|_: &str| {}
    } else {
        &|step: &str| println!("{step}")
    };
    let path = update::install(&app.http, release, asset, &app.paths, report_step).await?;

    if app.json {
        print_json(&report(&current, release, true, true, preview));
    } else {
        println!("installed wwnz {} to {}", release.version, path.display());
        println!("release notes: {}", release.url);
    }
    Ok(true)
}

/// What is on offer when nothing was built for this machine. The release
/// workflow builds on the platforms it is configured for, which need not
/// include the one asking.
fn no_asset_for_host(release: &Release) -> String {
    let built = release.platforms();
    let host = update::host_platforms()
        .first()
        .cloned()
        .unwrap_or_default();
    if built.is_empty() {
        format!("it publishes no binaries. Build from source, or install with `cargo install`. (host: {host})")
    } else {
        format!(
            "there is no {host} binary in it; the release has {}. \
             Build from source, or install with `cargo install`.",
            built.join(", ")
        )
    }
}

fn report(
    current: &semver::Version,
    release: &Release,
    available: bool,
    installed: bool,
    preview: Option<&Release>,
) -> serde_json::Value {
    serde_json::json!({
        "current": current.to_string(),
        "latest": release.version.to_string(),
        "preview": preview.map(|p| p.version.to_string()),
        "tag": release.tag,
        "url": release.url,
        "update_available": available,
        "installed": installed,
        "platform": update::host_platforms().first(),
        "asset": release.asset_for_host().map(|a| a.name.clone()),
        "binary": build::exe_path().map(|p| p.display().to_string()),
    })
}
