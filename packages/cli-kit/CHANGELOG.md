# Changelog

## cli-kit/v0.1.0 (2026-09-03)

### Features

- expose each crate's own version
  One subject for seven scopes because it is one line in each, and the reason
  is the same everywhere: `env!` expands where it is written, so a consumer
  asking for `CARGO_PKG_VERSION` gets its own version back. A crate that wants
  to report what it was built against has to be told by the crate itself.

- re-export comfy-table and serde_json
  A consumer building rows for `table()` or overriding `View::json` names
  those crates' types in code this one returns. Two majors of either in one
  dependency tree would surface as a baffling trait mismatch rather than as a
  version error, and every crate here has its own lockfile.

- output routing, tables, prompts, health reports
  Both existing CLIs choose between human output and --json with an
  early-return if in every command function, so the two paths drift and
  neither is testable without running the binary. One emit() and a View per
  thing replaces that, and Out can be pointed at a buffer, so a renderer is
  an ordinary unit test.

  View requires Serialize as a supertrait, so the JSON half cannot silently
  diverge from the type it claims to describe. Colour is refused outright
  for JSON: escape codes would make the document unparseable.

  No domain type here and there must never be one -- that is what the crate
  is for. Prompts write to stderr so they are never inside the document
  --json is producing.
