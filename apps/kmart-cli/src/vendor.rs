//! Keeping the storefront's own identifiers current.
//!
//! The Constructor index key and the Auth0 application id are Kmart's to
//! change, and the day they do, a version of this tool that carries them as
//! constants stops working until someone ships a new one. So they are read
//! from the storefront -- the same home page a browser reads them from -- and
//! cached.
//!
//! Three rules, each earning its place:
//!
//! * **Cached.** One request per country per week, not one per command.
//! * **Never fatal.** A fetch that fails or a page whose shape has moved
//!   leaves the values that shipped in [`kmart_api::vendor`] in charge. A bad
//!   afternoon at Kmart's CDN should not stop a search working.
//! * **Never chatty.** The refresh is a side effect of a command that was
//!   asked for something else, so it says nothing unless `--debug` is on.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kmart_api::{Country, Vendor};
use serde::{Deserialize, Serialize};

/// How long a cached set is trusted.
///
/// A week, because these change on the order of never and the cost of being
/// wrong for a few days is nil -- the baked fallback covers the same ground.
const MAX_AGE_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Serialize, Deserialize)]
struct Cached {
    fetched_at: u64,
    vendor: Vendor,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn path(state_dir: &Path, country: Country) -> PathBuf {
    state_dir.join(format!("vendor-{}.json", country.code().to_lowercase()))
}

/// Where a set of values came from. For `doctor`, and for `--debug`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Read from the storefront during this run.
    Fetched,
    /// Read from the storefront recently enough to still be trusted.
    Cached,
    /// Compiled in: either nothing has been fetched yet, or the attempt failed.
    Baked,
}

/// The values to use for a country, and where they came from.
///
/// Never returns an error: every failure resolves to [`Source::Baked`].
pub async fn resolve(
    http: &net_kit::wreq::Client,
    state_dir: &Path,
    country: Country,
    origin: &str,
    trace: impl Fn(&str),
) -> (Vendor, Source) {
    let file = path(state_dir, country);
    if let Some(cached) = read_cache(&file) {
        if now().saturating_sub(cached.fetched_at) < MAX_AGE_SECS {
            return (cached.vendor, Source::Cached);
        }
    }

    trace(&format!("reading the front-end config from {origin}/"));
    match fetch(http, origin, country).await {
        Some(vendor) => {
            // A write that fails is not worth reporting: the values are good
            // for this run either way, and the next run will fetch again.
            let _ = write_cache(&file, &vendor);
            trace("front-end config read");
            (vendor, Source::Fetched)
        }
        None => {
            trace("could not read it; using the values that shipped");
            // A stale cache still beats the baked set -- it was true more
            // recently -- so it is preferred over falling all the way back.
            match read_cache(&file) {
                Some(stale) => (stale.vendor, Source::Cached),
                None => (Vendor::baked(country), Source::Baked),
            }
        }
    }
}

/// The cached set for a country, however old, without fetching anything.
///
/// For the paths that must not make a request of their own -- picking the
/// Auth0 application a stored session was minted under, which happens for a
/// country the command may not even be asking about.
pub fn cached(state_dir: &Path, country: Country) -> Vendor {
    read_cache(&path(state_dir, country))
        .map(|c| c.vendor)
        .unwrap_or_else(|| Vendor::baked(country))
}

async fn fetch(http: &net_kit::wreq::Client, origin: &str, country: Country) -> Option<Vendor> {
    let url = format!("{origin}/");
    let res = http.get(&url).send().await.ok()?;
    let body = res.text().await.ok()?;
    Vendor::read(&body, country)
}

fn read_cache(file: &Path) -> Option<Cached> {
    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
}

fn write_cache(file: &Path, vendor: &Vendor) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let cached = Cached {
        fetched_at: now(),
        vendor: vendor.clone(),
    };
    std::fs::write(
        file,
        serde_json::to_string_pretty(&cached).unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
    }

    #[test]
    fn nothing_cached_falls_back_to_what_shipped() {
        let d = dir();
        assert_eq!(cached(d.path(), Country::Nz), Vendor::baked(Country::Nz));
    }

    #[test]
    fn a_cached_set_is_preferred_over_what_shipped() {
        let d = dir();
        let mut fresh = Vendor::baked(Country::Nz);
        fresh.search_key = "key_RotatedSinceRelease".into();
        write_cache(&path(d.path(), Country::Nz), &fresh).unwrap();
        assert_eq!(cached(d.path(), Country::Nz).search_key, fresh.search_key);
        // And it is per country, not one file for both.
        assert_eq!(cached(d.path(), Country::Au), Vendor::baked(Country::Au));
    }

    /// An address with nothing behind it, to stand for Kmart being
    /// unreachable. Port 1 is reserved and never listening.
    const NOWHERE: &str = "http://127.0.0.1:1";

    fn http() -> net_kit::wreq::Client {
        net_kit::http::build(kmart_api::client_spec()).unwrap()
    }

    #[tokio::test]
    async fn an_unreachable_storefront_is_not_fatal() {
        // The whole point of keeping the baked set. A search must work on a
        // day Kmart does not.
        let d = dir();
        let (vendor, source) = resolve(&http(), d.path(), Country::Nz, NOWHERE, |_| {}).await;
        assert_eq!(source, Source::Baked);
        assert_eq!(vendor, Vendor::baked(Country::Nz));
    }

    #[tokio::test]
    async fn a_stale_cache_beats_the_baked_set_when_the_fetch_fails() {
        // It was true more recently. Falling all the way back would undo a
        // rotation this tool had already learned about.
        let d = dir();
        let mut rotated = Vendor::baked(Country::Nz);
        rotated.search_key = "key_RotatedSinceRelease".into();
        std::fs::write(
            path(d.path(), Country::Nz),
            serde_json::to_string(&Cached {
                fetched_at: now() - MAX_AGE_SECS - 1,
                vendor: rotated.clone(),
            })
            .unwrap(),
        )
        .unwrap();

        let (vendor, source) = resolve(&http(), d.path(), Country::Nz, NOWHERE, |_| {}).await;
        assert_eq!(source, Source::Cached);
        assert_eq!(vendor.search_key, rotated.search_key);
    }

    #[tokio::test]
    async fn a_fresh_cache_costs_no_request_at_all() {
        let d = dir();
        write_cache(&path(d.path(), Country::Nz), &Vendor::baked(Country::Nz)).unwrap();
        // NOWHERE would fail if it were reached, so Cached proves it was not.
        let (_, source) = resolve(&http(), d.path(), Country::Nz, NOWHERE, |_| {}).await;
        assert_eq!(source, Source::Cached);
    }

    #[test]
    fn a_corrupt_cache_is_ignored_rather_than_fatal() {
        // Reaching for a half-written file should cost a fetch, not a crash.
        let d = dir();
        std::fs::write(path(d.path(), Country::Nz), "{not json").unwrap();
        assert_eq!(cached(d.path(), Country::Nz), Vendor::baked(Country::Nz));
    }
}
