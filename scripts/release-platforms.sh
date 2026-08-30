#!/usr/bin/env bash
# Print the GitHub runners a project's releases are built on.
#
# Usage:
#   scripts/release-platforms.sh <project>          # one runner label per line
#   scripts/release-platforms.sh <project> --json   # JSON array, for the matrix
#
# A project that ships binaries for more than one platform says so with an
# optional `release-platforms` target printing one runner label per line:
#
#   release-platforms:
#   	@echo ubuntu-latest
#   	@echo macos-14
#
# Unlike scripts/project-version.sh, which exits 3 to say "no answer here",
# this always has one: a project that declares nothing builds on ubuntu-latest
# alone, which is what every release did before the target existed. A project
# that *does* declare the target but fails to run it is an error rather than a
# silent fallback -- quietly dropping a platform would publish a release
# missing binaries nobody notices until someone tries to download one.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
name="${1:-}"
format="${2:---plain}"

if [[ -z "$name" ]]; then
  echo "usage: $(basename "$0") <project> [--plain|--json]" >&2
  exit 2
fi

dir="$repo_root/projects/$name"
if [[ ! -f "$dir/Makefile" ]]; then
  echo "error: no such project: $name" >&2
  exit 1
fi

target="release-platforms"
declared=false
# The same probe the fan-out uses: `make -n` can fail for reasons other than a
# missing target, so a miss is confirmed against the Makefile itself.
if make -C "$dir" -n "$target" >/dev/null 2>&1; then
  declared=true
elif grep -qE "^\.?PHONY.*\b${target}\b|^${target}:" "$dir/Makefile"; then
  declared=true
fi

platforms=()
if [[ "$declared" == true ]]; then
  if ! output="$(make -s -C "$dir" "$target" 2>&1)"; then
    echo "error: $name declares '$target' but running it failed:" >&2
    echo "$output" >&2
    exit 1
  fi
  while read -r label; do
    label="${label//[[:space:]]/}"
    [[ -z "$label" ]] && continue
    # This goes straight into `runs-on`, so keep it to what a runner label is.
    if [[ ! "$label" =~ ^[A-Za-z0-9._-]+$ ]]; then
      echo "error: $name's '$target' printed an implausible runner: $label" >&2
      exit 1
    fi
    # Declaring the same runner twice would build it twice and collide on the
    # artifact name.
    for seen in ${platforms[@]+"${platforms[@]}"}; do
      [[ "$seen" == "$label" ]] && continue 2
    done
    platforms+=("$label")
  done <<< "$output"

  if [[ ${#platforms[@]} -eq 0 ]]; then
    echo "error: $name's '$target' printed no runners" >&2
    exit 1
  fi
else
  platforms=("ubuntu-latest")
fi

case "$format" in
  --plain)
    printf '%s\n' "${platforms[@]}"
    ;;
  --json)
    out=""
    for p in "${platforms[@]}"; do
      [[ -n "$out" ]] && out+=","
      out+="\"$p\""
    done
    printf '[%s]\n' "$out"
    ;;
  *)
    echo "usage: $(basename "$0") <project> [--plain|--json]" >&2
    exit 2
    ;;
esac
