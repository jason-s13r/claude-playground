//! The only place in this program that reads the environment.
//!
//! Every library under `packages/` takes plain values; a `clippy.toml` in each
//! forbids `std::env::var` outright. That rule has to end somewhere, and it
//! ends here: one struct, read once, before anything is spawned. Below this
//! boundary the program is a function of its arguments.
//!
//! Two variables are missing on purpose. `GSNZ_RETAILER` and `GSNZ_TOKEN` are
//! `clap(env = ..)` attributes on the flags they back, read during
//! `Cli::parse()` -- the same single-threaded moment, expressed where the flag
//! is defined so `--help` documents it.

use std::path::PathBuf;

/// What the environment says, before flags and config have their turn.
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub config_dir: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
    pub secret_backend: Option<String>,
    pub no_color: bool,
    /// The login shell's path, which is how `completions` guesses which script
    /// to write when none is named.
    pub shell: Option<String>,
    pub newworld: Retailer,
    pub paknsave: Retailer,
    pub woolworths: Retailer,
    pub clubplus: ClubPlus,
}

/// The escape hatches for one retailer. These exist so the integration suite
/// can point the binary at a mock server, and so a broken default host can be
/// worked around without a release. The real hostnames live in the api crates.
#[derive(Clone, Debug, Default)]
pub struct Retailer {
    pub origin: Option<String>,
    /// The API host, where it differs from the storefront. Woolworths serves
    /// both from one origin, so it has none.
    pub api: Option<String>,
    pub store_id: Option<String>,
    pub token: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ClubPlus {
    pub origin: Option<String>,
    pub api: Option<String>,
}

impl Overrides {
    pub fn read() -> Overrides {
        Overrides {
            config_dir: path("GSNZ_CONFIG_DIR"),
            state_dir: path("GSNZ_STATE_DIR"),
            secret_backend: var("GSNZ_SECRET_BACKEND"),
            // Set at all, to anything, means no colour. That is what the
            // convention says, so an empty value is not an override.
            no_color: std::env::var_os("NO_COLOR").is_some(),
            shell: var("SHELL"),
            newworld: Retailer::read("GSNZ_NEWWORLD"),
            paknsave: Retailer::read("GSNZ_PAKNSAVE"),
            woolworths: Retailer {
                origin: var("GSNZ_WOOLWORTHS_ORIGIN"),
                // Auth0 is a separate host, so Woolworths' second origin is an
                // auth one rather than an API one.
                api: var("GSNZ_WOOLWORTHS_AUTH_ORIGIN"),
                store_id: var("GSNZ_WOOLWORTHS_STORE_ID"),
                token: None,
            },
            clubplus: ClubPlus {
                origin: var("GSNZ_CLUBPLUS_ORIGIN"),
                api: var("GSNZ_CLUBPLUS_API"),
            },
        }
    }
}

impl Retailer {
    fn read(prefix: &str) -> Retailer {
        Retailer {
            origin: var(&format!("{prefix}_ORIGIN")),
            api: var(&format!("{prefix}_API")),
            store_id: var(&format!("{prefix}_STORE_ID")),
            token: var(&format!("{prefix}_TOKEN")),
        }
    }
}

/// An empty variable is treated as unset: `GSNZ_STATE_DIR=` in a shell script
/// means "I did not set this", not "use the current directory".
fn var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn path(name: &str) -> Option<PathBuf> {
    var(name).map(PathBuf::from)
}
