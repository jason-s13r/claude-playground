#!/usr/bin/env bash
# Print the build matrix for the packages this run would release.
#
# Usage: scripts/release-matrix.sh
#
# dispat plans the release and runs each package's stages, but it runs them on
# one machine. A project shipping binaries for several platforms needs one
# runner per platform, and that is the one thing the release workflow still has
# to arrange itself. This turns "what is releasing" into "what has to be built,
# and where".
#
# A project declares its runners in its own dispat.json, under `custom` -- data
# dispat carries but does not interpret:
#
#   "custom": { "releasePlatforms": ["ubuntu-latest", "macos-14"] }
#
# A project that declares nothing builds on ubuntu-latest alone. Output is the
# JSON GitHub Actions wants for `strategy.matrix` -- an object with an
# `include` list -- and an empty object when the run would release nothing.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

dispat status --log-format json 2>/dev/null | python3 -c '
import json, os, sys

rows = []
for line in sys.stdin:
    try:
        rec = json.loads(line)
    except ValueError:
        continue
    # A releasing package carries the bump it earned; an unchanged one does not.
    name = rec.get("package")
    if not name or not rec.get("bump") or rec.get("bump") == "none":
        continue

    platforms = ["ubuntu-latest"]
    cfg = os.path.join("projects", name, "dispat.json")
    if os.path.exists(cfg):
        with open(cfg) as fh:
            declared = json.load(fh).get("custom", {}).get("releasePlatforms")
        if declared:
            platforms = declared

    for platform in platforms:
        rows.append({"package": name, "platform": platform})

json.dump({"include": rows} if rows else {}, sys.stdout)
'
