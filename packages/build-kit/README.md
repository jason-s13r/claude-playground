# build-kit

What a binary is, where it came from, and how it replaces itself.

Two halves, because they run at different times and want different
dependencies:

- **`emit`**, used from a consumer's `build.rs`, stamps provenance in. `std`
  only, behind the `emit` feature.
- **the runtime half** (the default `runtime` feature) reads that stamp back,
  finds newer releases and installs them.

A build script that pulled in the whole HTTP stack would have to build it before
the crate that needs it, so the split is a feature gate:

```toml
build-kit = { path = "../../packages/build-kit", version = "^0.1.0" }

[build-dependencies]
build-kit = { path = "../../packages/build-kit", version = "^0.1.0", default-features = false, features = ["emit"] }
```

Both entries stay **inline**, on one line each. dispat rewrites the version of
an inline entry on release and does not see the `[build-dependencies.build-kit]`
sub-table form, which leaves it pinned to a version that no longer exists — see
[`packages/README.md`](../README.md#depending-on-one).

## The `env!` problem

`env!` expands in the crate where it is *written*, and a `cargo:rustc-env` set
by an app's build script is only visible while compiling that app. So this crate
can never `env!("GSNZ_VERSION")` on a consumer's behalf.

Instead `Stamper` writes a Rust source file into the consumer's `OUT_DIR`, and
the consumer includes it:

```rust
// build.rs
fn main() {
    build_kit::emit::Stamper::new("GSNZ")
        .tag_glob("grocery-nz-cli/v*")
        .emit()
        .expect("stamping the build");
}

// src/build.rs
include!(concat!(env!("OUT_DIR"), "/build_stamp.rs"));  // defines STAMP
```

`OUT_DIR` is per-crate, so the `env!` resolves in the right place. The payoff
beyond correctness: [`Stamp`](src/stamp.rs) is an ordinary struct, so version
strings and dates are unit-testable without compiling a binary to test them
against.

`emit` is also the one place the crate-wide ban on reading the environment is
lifted. A build script's input *is* its environment, and it runs
single-threaded before anything else exists.

## What the stamp carries

Version, commit, commit date, tag, repo, builder, rustc, target and profile —
each degrading to `""`, so a source tarball with no `.git` and no `git` on PATH
still builds and just reports less.

Two fields are subtler than they look. `version` comes from the release rather
than `CARGO_PKG_VERSION`, because a release is compiled *before* the commit that
bumps the manifest, so the manifest is stale at that moment. And `builder` is
set only by a release workflow, which is what separates a published binary from
one somebody built on a laptop that happened to have the tag checked out.

`short_version` is the `-V` line; `long_version` is the whole provenance,
including how the file got installed and — passed in by the app — the versions
of the libraries it was compiled against.

## Updating

Releases here live in a monorepo, one tag namespace per project
(`<project>/vX.Y.Z`), and every project releases on its own schedule. That rules
out GitHub's `releases/latest`, which answers with the newest release of
*anything* in the repository — usually somebody else's project. So `update`
lists releases and picks the newest tag within one namespace itself.

```rust
let src = update::Source::new("owner/repo", "grocery-nz-cli", STAMP.version);
let releases = update::releases(&http, &src).await?;
if let Some(release) = update::pick(&releases, &current, false) {
    let asset = release.asset_for_host().ok_or(...)?;
    update::install(&http, &src, release, asset, &exe, &report).await?;
}
```

An install is deliberately paranoid, in this order: stage a temporary file
*beside* the binary it will replace, so the final swap is a rename within one
filesystem and an unwritable install directory fails in a second rather than
after several megabytes; download; verify the SHA-256 against the release's
`SHA256SUMS`, refusing outright when the release has none; extract; then swap.

[`install`](src/install.rs) writes a small record of where the binary came from,
so a later `--version` can say it was installed by `update` rather than built or
packaged.

## Development

```bash
dispat run check --since all -p build-kit
```

`check` builds twice — once normally, once as `--no-default-features --features
emit` — because the build-script half is what a consumer's `build.rs` compiles,
and nothing else would catch a runtime dependency leaking into it.

Used by all three apps. Not published to crates.io; consumers declare a path
dependency, as [`packages/README.md`](../README.md) describes.
