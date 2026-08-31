#!/usr/bin/env bash
# Produce this release's artifacts and tell dispat which files to attach.
#
# dispat runs this as the build stage, from this directory, with the release's
# identity in the environment. There is no publish stage: nothing here goes to
# a registry, so the whole delivery is the git tag and the GitHub release.
#
# `fsnz update` verifies a download against a single SHA256SUMS asset and
# refuses to install when it holds no line for the file. So SHA256SUMS has to
# cover every platform, which decides how this works: there is no Rust host
# that builds both the Linux and the macOS binary, so CI builds each on its own
# runner and stages them here, and only then are the checksums written.
#
# STAGING_ROOT names the directory the release job downloads those runners'
# artifacts into, one sub-directory per package. When this package has one
# there, its contents are the release; otherwise this is a local run and this
# host's own binary is the whole release.
set -euo pipefail

: "${DISPAT_PACKAGE:?must be run by dispat}"
: "${DISPAT_NEW_VERSION:?must be run by dispat}"
: "${DISPAT_OUTPUT:?must be run by dispat}"

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"

# Built fresh every time. `tar` only ever adds, so a leftover tarball from an
# earlier version would otherwise still be here at upload time and would ship
# as though this build had produced it.
rm -rf release
mkdir -p release

staged="${STAGING_ROOT:-}/${DISPAT_PACKAGE}"
if [[ -n "${STAGING_ROOT:-}" && -d "$staged" ]]; then
  while IFS= read -r -d '' file; do
    base="$(basename "$file")"
    # A clash means a runner ignored the platform it was on, and one binary
    # would silently overwrite another.
    if [[ -e "release/$base" ]]; then
      echo "error: two platforms both produced $base" >&2
      exit 1
    fi
    cp "$file" "release/$base"
  done < <(find "$staged" -type f -print0)
else
  cargo build --release
  tar -czf "release/$DISPAT_PACKAGE-$DISPAT_NEW_VERSION-$os-$arch.tar.gz" -C target/release fsnz
fi

shopt -s nullglob
artifacts=(release/*)
shopt -u nullglob
if [[ ${#artifacts[@]} -eq 0 ]]; then
  echo "error: nothing to release for $DISPAT_PACKAGE $DISPAT_NEW_VERSION" >&2
  exit 1
fi

# One SHA256SUMS over every platform's files, written by one implementation.
sums() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
( cd release && sums -- * > SHA256SUMS )

# dispat uploads the whitespace-separated absolute paths this names. $PWD is
# this package's folder inside a stage, so the glob is absolute for free.
echo "DISPAT_EXPORT_GITHUB=$(ls -d "$PWD/release/"* | tr '\n' ' ')" >> "$DISPAT_OUTPUT"
echo "attaching $(ls -1 release | wc -l | tr -d ' ') asset(s) to $DISPAT_PACKAGE $DISPAT_NEW_VERSION"
