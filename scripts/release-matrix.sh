#!/usr/bin/env bash
# Print the build matrix for the packages this run has to build binaries for.
#
# Usage: scripts/release-matrix.sh [--count]
#
# dispat runs a package's stages on one machine, so a project shipping binaries
# for several platforms needs one runner each. It declares them in its own
# dispat.yaml under `custom`, which dispat carries but does not read:
#
#   custom:
#     releasePlatforms: [ubuntu-latest, macos-14]
#
# Declaring nothing means ubuntu-latest alone.
#
# A package that defines no `release-build` script gets no row: a library ships
# no binary, so there is nothing for a runner to build and nothing to upload.
# It still releases -- a tag and an assetless GitHub release -- which is why
# `--count` reports every releasing package rather than only the rows. The
# release job needs that number: a run that releases only libraries has an
# empty matrix and must still publish.
#
# Output is a `strategy.matrix` object, or `{}` when nothing needs building.
# Each row carries the package's directory as well, since a package may live in
# either space.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

dispat status --log-format json 2>/dev/null | python3 -c '
import json, os, sys

try:
    import yaml
except ImportError:
    sys.exit("release-matrix.sh needs PyYAML: pip install pyyaml")

count_only = "--count" in sys.argv[1:]

rows = []
releasing = 0
for line in sys.stdin:
    try:
        rec = json.loads(line)
    except ValueError:
        continue
    # A releasing package carries a bump; an unchanged one does not.
    name = rec.get("package")
    if not name or not rec.get("bump") or rec.get("bump") == "none":
        continue
    releasing += 1

    # Every space is named for the directory it lives in, so this is the path.
    directory = os.path.join(rec.get("space", "apps"), name)

    cfg = os.path.join(directory, "dispat.yaml")
    declared = None
    builds = False
    if os.path.exists(cfg):
        with open(cfg) as fh:
            parsed = yaml.safe_load(fh) or {}
        # No release-build means no artifact, so no runner has anything to do.
        builds = "release-build" in (parsed.get("scripts") or {})
        declared = (parsed.get("custom") or {}).get("releasePlatforms")
    if not builds:
        continue

    for platform in declared or ["ubuntu-latest"]:
        rows.append({"package": name, "dir": directory, "platform": platform})

if count_only:
    print(releasing)
else:
    json.dump({"include": rows} if rows else {}, sys.stdout)
' -- "$@"
