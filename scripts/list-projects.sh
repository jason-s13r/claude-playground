#!/usr/bin/env bash
# List every project in the monorepo.
#
# A "project" is any directory directly under projects/ that contains a
# Makefile. The Makefile is the uniform entry point -- whatever the project is
# written in, the rest of the repo (and CI) only ever talks to it via make.
#
# Usage:
#   scripts/list-projects.sh          # one name per line
#   scripts/list-projects.sh --json   # JSON array, for the CI matrix
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
projects_dir="$repo_root/projects"

names=()
if [[ -d "$projects_dir" ]]; then
  for makefile in "$projects_dir"/*/Makefile; do
    [[ -e "$makefile" ]] || continue
    dir="$(dirname "$makefile")"
    names+=("$(basename "$dir")")
  done
fi

case "${1:---plain}" in
  --plain)
    printf '%s\n' ${names[@]+"${names[@]}"}
    ;;
  --json)
    out=""
    for n in ${names[@]+"${names[@]}"}; do
      [[ -n "$out" ]] && out+=","
      out+="\"$n\""
    done
    printf '[%s]\n' "$out"
    ;;
  *)
    echo "usage: $(basename "$0") [--plain|--json]" >&2
    exit 2
    ;;
esac
