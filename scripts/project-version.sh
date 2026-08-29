#!/usr/bin/env bash
# Print the version a project declares in its own manifest, if it declares one.
#
# Usage: scripts/project-version.sh <project>
#
# Prints the version and exits 0 when found; prints nothing and exits 3 when
# the project has no manifest to read (C and plain-Makefile projects). The
# release workflow uses this to check that a release tag agrees with what the
# project actually says its version is, so artifacts cannot ship mislabelled.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
name="${1:-}"

if [[ -z "$name" ]]; then
  echo "usage: $(basename "$0") <project>" >&2
  exit 2
fi

dir="$repo_root/projects/$name"
if [[ ! -d "$dir" ]]; then
  echo "error: no such project: $name" >&2
  exit 1
fi

# TOML: the version key of a given table, stopping at the next table header.
toml_version() {
  sed -n "/^\[$2\]/,/^\[/p" "$1" | grep -m1 -E '^[[:space:]]*version[[:space:]]*=' | cut -d'"' -f2
}

if [[ -f "$dir/Cargo.toml" ]]; then
  toml_version "$dir/Cargo.toml" package
elif [[ -f "$dir/package.json" ]]; then
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("version",""))' "$dir/package.json"
elif [[ -f "$dir/pyproject.toml" ]]; then
  toml_version "$dir/pyproject.toml" project
else
  exit 3
fi
