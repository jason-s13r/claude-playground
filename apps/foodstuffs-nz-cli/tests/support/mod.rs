//! Running the binary the way a shell would, minus the developer's own
//! settings.
// Each integration test binary compiles this module but uses only part of it.
#![allow(dead_code)]

use assert_cmd::Command;
use tempfile::TempDir;

/// Every variable `fsnz` reads. Listed rather than filtered by prefix so that
/// adding one to `src/env.rs` without adding it here is a test that fails, not
/// a test that quietly starts depending on the developer's shell.
pub const READS: [&str; 17] = [
    "FSNZ_CONFIG_DIR",
    "FSNZ_STATE_DIR",
    "FSNZ_SECRET_BACKEND",
    "FSNZ_UPDATE_API",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "FSNZ_BANNER",
    "FSNZ_NEWWORLD_ORIGIN",
    "FSNZ_NEWWORLD_API",
    "FSNZ_NEWWORLD_STORE_ID",
    "FSNZ_NEWWORLD_TOKEN",
    "FSNZ_PAKNSAVE_ORIGIN",
    "FSNZ_PAKNSAVE_API",
    "FSNZ_PAKNSAVE_STORE_ID",
    "FSNZ_PAKNSAVE_TOKEN",
    "FSNZ_CLUBPLUS_ORIGIN",
    "FSNZ_CLUBPLUS_API",
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

    /// A `fsnz` that cannot reach the network, the developer's config or the
    /// developer's credential store.
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("fsnz").expect("the fsnz binary");
        for name in READS {
            cmd.env_remove(name);
        }
        // `NO_COLOR` is read straight from the environment, not through READS.
        cmd.env_remove("NO_COLOR");
        cmd.env_remove("SHELL");
        let dir = self.home.path();
        cmd.env("FSNZ_CONFIG_DIR", dir)
            .env("FSNZ_STATE_DIR", dir.join("state"))
            // Never the real keyring: a test must not be able to read or write
            // the machine's credentials.
            .env("FSNZ_SECRET_BACKEND", "file")
            .env("NO_COLOR", "1");
        // Every host, pointed at a closed port. Nothing in this suite may
        // reach Foodstuffs: `doctor` probes each banner, and without this it
        // would make real calls per test that runs it -- slow, flaky, and
        // traffic nobody asked for.
        for name in [
            "FSNZ_NEWWORLD_ORIGIN",
            "FSNZ_NEWWORLD_API",
            "FSNZ_PAKNSAVE_ORIGIN",
            "FSNZ_PAKNSAVE_API",
            "FSNZ_CLUBPLUS_ORIGIN",
            "FSNZ_CLUBPLUS_API",
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
