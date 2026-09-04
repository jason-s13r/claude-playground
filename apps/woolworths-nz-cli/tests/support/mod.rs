//! Running the binary the way a shell would, minus the developer's own
//! settings.
// Each integration test binary compiles this module but uses only part of it.
#![allow(dead_code)]

use assert_cmd::Command;
use tempfile::TempDir;

/// Every variable `wwnz` reads. Listed rather than filtered by prefix so that
/// adding one to `src/env.rs` without adding it here is a test that fails, not
/// a test that quietly starts depending on the developer's shell.
pub const READS: [&str; 9] = [
    "WWNZ_CONFIG_DIR",
    "WWNZ_STATE_DIR",
    "WWNZ_SECRET_BACKEND",
    "WWNZ_UPDATE_API",
    "WWNZ_DEBUG_AUTH",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "WWNZ_ORIGIN",
    "WWNZ_AUTH_ORIGIN",
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

    /// A `wwnz` that cannot reach the network, the developer's config or the
    /// developer's credential store.
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("wwnz").expect("the wwnz binary");
        for name in READS {
            cmd.env_remove(name);
        }
        // `NO_COLOR` is read straight from the environment, not through READS.
        cmd.env_remove("NO_COLOR");
        cmd.env_remove("SHELL");
        let dir = self.home.path();
        cmd.env("WWNZ_CONFIG_DIR", dir)
            .env("WWNZ_STATE_DIR", dir.join("state"))
            // Never the real keyring: a test must not be able to read or write
            // the machine's credentials.
            .env("WWNZ_SECRET_BACKEND", "file")
            .env("NO_COLOR", "1");
        // Both hosts, pointed at a closed port. Nothing in this suite may reach
        // Woolworths: `doctor` probes the storefront, and without this it would
        // make a real call per test that runs it -- slow, flaky, and traffic
        // nobody asked for.
        for name in ["WWNZ_ORIGIN", "WWNZ_AUTH_ORIGIN"] {
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
