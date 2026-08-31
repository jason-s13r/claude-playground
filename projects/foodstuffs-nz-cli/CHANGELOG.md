# Changelog

## foodstuffs-nz-cli/v0.2.0-rc.3 (2026-08-31)

### Fixes

- keep Cargo.lock in step with the released version
  The release job never runs cargo -- its build stage collects what the matrix
  runners already built -- so nothing rewrote the lock after the version stage
  bumped the manifest, and the release commit had no lock change to carry. main
  drifted a release further each time.

  autoVersion.syncLock runs between the version and build stages, which is where
  that regeneration belongs. `cargo metadata` rewrites the package's own version
  and nothing else. Offline first, since a warm cache makes it free; a fresh
  runner has no registry to read and falls back to the network.


## foodstuffs-nz-cli/v0.2.0-rc.2 (2026-08-31)

### Fixes

- drop the dirty marker from the build stamp
  A release build rewrites Cargo.lock as it runs, so whether `git status` saw the
  tree clean depended on whether it ran before or after cargo got there. Two
  releases off the same pipeline disagreed, and 0.2.0-rc.1 shipped branding
  itself as built from edited source.

  `built by` already separates a released binary from a hand-built one, which is
  the distinction that carries weight.


## foodstuffs-nz-cli/v0.2.0-rc.1 (2026-08-31)

### Fixes

- cut a candidate to exercise self-update


## foodstuffs-nz-cli/v0.2.0-rc.0 (2026-08-31)

### Features

- follow the release channel the running build is on
  `fsnz update` skipped every prerelease, so a preview build was stranded: it
  could not reach the next preview, and would not move until a stable release
  passed it.

  The channel now follows from the running version and is not stored. A stable
  build still only sees stable releases. A preview build takes a newer stable
  when one exists, and otherwise walks on through the previews, so it rejoins the
  stable channel by itself.

  `--pre-release` takes the newest release of either channel. A version argument
  installs exactly that release, downgrades included, with the leading `v`
  optional; it pins nothing, so the next plain update follows the rules above.
  `--check` mentions a preview without counting it, so it cannot flip the exit
  code a script gates on.


## foodstuffs-nz-cli/v0.1.4-rc.1 (2026-08-31)

### Fixes

- stamp the released version into the binary
  The build matrix runs `dispat run`, which has no version stage, so the
  checked-out Cargo.toml still carries the previous release. Every published
  binary reported it: the 0.1.4-rc.0 tarball holds a binary saying 0.1.3, with
  no release tag. build.rs now prefers DISPAT_NEW_VERSION and DISPAT_TAG,
  falling back to the manifest and `git describe` outside a release.

  The update test mounted its "already newest" release at the running version,
  which a prerelease build filters back out; it now also mounts an older stable
  release, so it holds on either channel.


## foodstuffs-nz-cli/v0.1.4-rc.0 (2026-08-31)

### Fixes

- exercise the dispat release pipeline
