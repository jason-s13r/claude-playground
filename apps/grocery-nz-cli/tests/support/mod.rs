//! Running the binary the way a shell would, minus the developer's own
//! settings.
// Each integration test binary compiles this module but uses only part of it.
#![allow(dead_code)]

use assert_cmd::Command;
use tempfile::TempDir;

/// Every variable `gsnz` reads. Listed rather than filtered by prefix so that
/// adding one to `src/env.rs` without adding it here is a test that fails, not
/// a test that quietly starts depending on the developer's shell.
pub const READS: [&str; 23] = [
    "GSNZ_CONFIG_DIR",
    "GSNZ_STATE_DIR",
    "GSNZ_SECRET_BACKEND",
    "GSNZ_UPDATE_API",
    "GSNZ_DEBUG_AUTH",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "GSNZ_RETAILER",
    "GSNZ_TOKEN",
    "GSNZ_NEWWORLD_ORIGIN",
    "GSNZ_NEWWORLD_API",
    "GSNZ_NEWWORLD_STORE_ID",
    "GSNZ_NEWWORLD_TOKEN",
    "GSNZ_PAKNSAVE_ORIGIN",
    "GSNZ_PAKNSAVE_API",
    "GSNZ_PAKNSAVE_STORE_ID",
    "GSNZ_PAKNSAVE_TOKEN",
    "GSNZ_WOOLWORTHS_ORIGIN",
    "GSNZ_WOOLWORTHS_AUTH_ORIGIN",
    "GSNZ_WOOLWORTHS_STORE_ID",
    "GSNZ_CLUBPLUS_ORIGIN",
    "GSNZ_CLUBPLUS_API",
    "NO_COLOR",
];

/// A port nothing listens on, so a connection is refused at once rather than
/// timing out.
pub const DEAD: &str = "http://127.0.0.1:1";

pub struct Sandbox {
    pub home: TempDir,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        Sandbox {
            home: TempDir::new().expect("temp dir"),
        }
    }

    /// A `gsnz` that cannot reach the network, the developer's config or the
    /// developer's credential store.
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("gsnz").expect("the gsnz binary");
        for name in READS {
            cmd.env_remove(name);
        }
        let dir = self.home.path();
        cmd.env("GSNZ_CONFIG_DIR", dir)
            .env("GSNZ_STATE_DIR", dir.join("state"))
            // Never the real keyring: a test must not be able to read or write
            // the machine's credentials.
            .env("GSNZ_SECRET_BACKEND", "file")
            .env("NO_COLOR", "1");
        // Every host, pointed at a closed port. Nothing in this suite may
        // reach a supermarket: `doctor` probes each shop, and without this it
        // would make three real calls per test that runs it -- slow, flaky,
        // and traffic nobody asked for.
        for name in [
            "GSNZ_NEWWORLD_ORIGIN",
            "GSNZ_NEWWORLD_API",
            "GSNZ_PAKNSAVE_ORIGIN",
            "GSNZ_PAKNSAVE_API",
            "GSNZ_WOOLWORTHS_ORIGIN",
            "GSNZ_WOOLWORTHS_AUTH_ORIGIN",
            "GSNZ_CLUBPLUS_ORIGIN",
            "GSNZ_CLUBPLUS_API",
        ] {
            cmd.env(name, DEAD);
        }
        cmd
    }

    pub fn write_config(&self, text: &str) {
        std::fs::write(self.home.path().join("config.toml"), text).expect("writing the config");
    }

    pub fn read_config(&self) -> String {
        std::fs::read_to_string(self.home.path().join("config.toml")).unwrap_or_default()
    }
}
