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
    pub update_api: Option<String>,
    pub github_token: Option<String>,
    /// Narrate the login flows on stderr. Nothing they print is a credential:
    /// query strings are dropped and cookies appear by name only.
    pub debug_auth: bool,
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
    /// Read once, and shared.
    ///
    /// `--version` needs the state directory to say how this binary was
    /// installed, and clap builds that string before `App` exists. Memoising
    /// keeps "the environment is read once" literally true rather than nearly
    /// true.
    pub fn get() -> &'static Overrides {
        static CELL: std::sync::OnceLock<Overrides> = std::sync::OnceLock::new();
        CELL.get_or_init(Overrides::read)
    }

    pub fn read() -> Overrides {
        Overrides {
            config_dir: path("GSNZ_CONFIG_DIR"),
            state_dir: path("GSNZ_STATE_DIR"),
            secret_backend: var("GSNZ_SECRET_BACKEND"),
            update_api: var("GSNZ_UPDATE_API"),
            // `gh` writes one and the Actions runner the other; either lifts
            // the anonymous rate limit on the release list.
            github_token: var("GITHUB_TOKEN").or_else(|| var("GH_TOKEN")),
            debug_auth: flag("GSNZ_DEBUG_AUTH"),
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

/// Set to anything but a denial means on: `GSNZ_DEBUG_AUTH=1` and
/// `GSNZ_DEBUG_AUTH=yes` should not need to be told apart.
fn flag(name: &str) -> bool {
    var(name).is_some_and(|v| !matches!(v.to_lowercase().as_str(), "0" | "false" | "no"))
}
