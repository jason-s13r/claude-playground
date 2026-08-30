//! `fsnz update` -- check for a newer release, and install it.

use anyhow::{bail, Result};

use crate::app::App;
use crate::build;
use crate::commands::io::print_json;
use crate::update::{self, Release};

/// Returns false when a newer release exists and was not installed, so
/// `fsnz update --check` can gate a script the way `doctor` does.
pub async fn run(app: &App, check: bool) -> Result<bool> {
    let current = update::current()?;
    let Some(release) = update::latest(&app.http).await? else {
        if app.json {
            print_json(&serde_json::json!({
                "current": current.to_string(),
                "latest": null,
                "update_available": false,
                "installed": false,
            }));
        } else {
            println!("{} has no published releases yet", build::VERSION);
        }
        return Ok(true);
    };

    let available = release.version > current;
    let asset = release.asset_for_host();

    if check || !available {
        if app.json {
            print_json(&report(&current, &release, available, false));
        } else if available {
            println!("fsnz {current} -> {} available", release.version);
            println!("  {}", release.url);
            match asset {
                Some(a) => println!("  run `fsnz update` to install {}", a.name),
                None => println!("  {}", no_asset_for_host(&release)),
            }
        } else {
            println!("fsnz {current} is the latest release");
        }
        return Ok(!available);
    }

    let Some(asset) = asset else {
        bail!(
            "fsnz {} is available, but {}\n  {}",
            release.version,
            no_asset_for_host(&release),
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
        println!("updating fsnz {current} -> {}", release.version);
    }
    let report_step: &dyn Fn(&str) = if app.json {
        &|_: &str| {}
    } else {
        &|step: &str| println!("{step}")
    };
    let path = update::install(&app.http, &release, asset, &app.paths, report_step).await?;

    if app.json {
        print_json(&report(&current, &release, true, true));
    } else {
        println!("installed fsnz {} to {}", release.version, path.display());
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
) -> serde_json::Value {
    serde_json::json!({
        "current": current.to_string(),
        "latest": release.version.to_string(),
        "tag": release.tag,
        "url": release.url,
        "update_available": available,
        "installed": installed,
        "platform": update::host_platforms().first(),
        "asset": release.asset_for_host().map(|a| a.name.clone()),
        "binary": build::exe_path().map(|p| p.display().to_string()),
    })
}
