#!/usr/bin/env bash
# Run one make target across a set of projects.
#
# Usage: scripts/fanout.sh <target> [project ...]
#
# Projects that do not define the requested target are skipped rather than
# failing the run -- a C project has no reason to implement `fmt-check` just
# because a Rust one does. Every other failure is real: the script keeps going
# so you see the full picture, then exits non-zero with a summary.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

target="${1:-}"
if [[ -z "$target" ]]; then
  echo "usage: $(basename "$0") <target> [project ...]" >&2
  exit 2
fi
shift

projects=("$@")
if [[ ${#projects[@]} -eq 0 ]]; then
  echo "No projects found. Create one with: make new TEMPLATE=<lang> NAME=<name>"
  exit 0
fi

if [[ -t 1 ]]; then
  bold=$'\033[1m'; green=$'\033[32m'; red=$'\033[31m'; dim=$'\033[2m'; reset=$'\033[0m'
else
  bold=""; green=""; red=""; dim=""; reset=""
fi

failed=()
skipped=()
passed=()

for name in "${projects[@]}"; do
  dir="$repo_root/projects/$name"
  if [[ ! -f "$dir/Makefile" ]]; then
    echo "${red}error:${reset} no such project: $name" >&2
    failed+=("$name")
    continue
  fi

  # Dry-run probe: if the target does not exist, skip the project quietly.
  if ! make -C "$dir" -n "$target" >/dev/null 2>&1; then
    if ! grep -qE "^\.?PHONY.*\b${target}\b|^${target}:" "$dir/Makefile"; then
      skipped+=("$name")
      echo "${dim}==> $name: no '$target' target, skipping${reset}"
      continue
    fi
  fi

  echo "${bold}==> $name: make $target${reset}"
  if make -C "$dir" "$target"; then
    passed+=("$name")
  else
    failed+=("$name")
    echo "${red}==> $name: FAILED${reset}"
  fi
  echo
done

printf '%s%s%s: %d ok' "$bold" "$target" "$reset" "${#passed[@]}"
[[ ${#skipped[@]} -gt 0 ]] && printf ', %d skipped' "${#skipped[@]}"
if [[ ${#failed[@]} -gt 0 ]]; then
  printf ', %s%d failed (%s)%s\n' "$red" "${#failed[@]}" "${failed[*]}" "$reset"
  exit 1
fi
printf ', %s0 failed%s\n' "$green" "$reset"
