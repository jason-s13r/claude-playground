#!/usr/bin/env bash
# Print the build matrix for the packages this run would release.
#
# Usage: scripts/release-matrix.sh
#
# dispat runs a package's stages on one machine, so a project shipping binaries
# for several platforms needs one runner each. It declares them in its own
# dispat.yaml under `custom`, which dispat carries but does not read:
#
#   custom:
#     releasePlatforms: [ubuntu-latest, macos-14]
#
# Declaring nothing means ubuntu-latest alone. Output is a `strategy.matrix`
# object, or `{}` when the run would release nothing. Each row carries the
# package's directory as well, since a package may live in either space.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

dispat status --log-format json 2>/dev/null | python3 -c '
import json, os, sys

try:
    import yaml
except ImportError:
    sys.exit("release-matrix.sh needs PyYAML: pip install pyyaml")

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

    # Every space is named for the directory it lives in, so this is the path.
    directory = os.path.join(rec.get("space", "apps"), name)

    platforms = ["ubuntu-latest"]
    cfg = os.path.join(directory, "dispat.yaml")
    if os.path.exists(cfg):
        with open(cfg) as fh:
            declared = (yaml.safe_load(fh) or {}).get("custom", {}).get("releasePlatforms")
        if declared:
            platforms = declared

    for platform in platforms:
        rows.append({"package": name, "dir": directory, "platform": platform})

json.dump({"include": rows} if rows else {}, sys.stdout)
'
