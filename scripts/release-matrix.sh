#!/usr/bin/env bash
# Print the build matrix for the packages this run would release.
#
# Usage: scripts/release-matrix.sh
#
# dispat runs a package's stages on one machine, so a project shipping binaries
# for several platforms needs one runner each. It declares them in its own
# dispat.json under `custom`, which dispat carries but does not read:
#
#   "custom": { "releasePlatforms": ["ubuntu-latest", "macos-14"] }
#
# Declaring nothing means ubuntu-latest alone. Output is a `strategy.matrix`
# object, or `{}` when the run would release nothing.
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
    # A releasing package carries a bump; an unchanged one does not.
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
