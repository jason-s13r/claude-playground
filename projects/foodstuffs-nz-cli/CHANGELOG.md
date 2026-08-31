# Changelog

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
