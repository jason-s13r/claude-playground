# Changelog

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
