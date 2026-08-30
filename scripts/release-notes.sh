#!/usr/bin/env bash
# Print the change list for a project's release, grouped by commit type.
#
# Usage: scripts/release-notes.sh <project> <range>
#
#   scripts/release-notes.sh foodstuffs-nz-cli foodstuffs-nz-cli/v0.1.2..HEAD
#
# Writes markdown to stdout and nothing else; the release workflow wraps it in
# the surrounding prose. Exits 0 with no output when the range holds nothing
# for the project, so the caller decides how to say "no changes".
#
# Which commits belong to a project is the union of two filters: commits that
# touched projects/<project>, and commits whose conventional-commit scope names
# it. The second filter is the only way a change that is about a project
# without living in its directory -- CI, this workflow, a shared script --
# reaches its notes.
#
# Subjects are then grouped by conventional-commit type, because a flat list
# makes a reader work out which lines are fixes and which are chores. Anything
# that is not a conventional commit is not mangled into a guess: it keeps its
# subject intact, which is what unscoped repo-wide work looks like here.
set -euo pipefail

project="${1:-}"
range="${2:-}"

if [[ -z "$project" || -z "$range" ]]; then
  echo "usage: $(basename "$0") <project> <range>" >&2
  exit 2
fi

# Groups, in the order they are printed: what a reader wants first, first.
# `key|heading`; the keys are matched against the parsed commit type, and
# `breaking` and `other` are assigned rather than parsed.
groups=(
  "breaking|Breaking changes"
  "feat|Features"
  "fix|Fixes"
  "perf|Performance"
  "docs|Documentation"
  "refactor|Refactoring"
  "test|Tests"
  "build|Build and CI"
  "other|Other changes"
)

# The group a conventional-commit type belongs to; some share a heading rather
# than earning their own. Housekeeping -- version bumps, formatting, reverts --
# lands in "Other changes" with the unscoped repo-wide work, since a `Chores`
# heading of its own is a section nobody reads. Empty for anything that is not
# a type we know, so a subject like `WIP: ...` is left alone instead of being
# read as a type.
group_for() {
  case "$1" in
    ci) echo build ;;
    chore|style|revert) echo other ;;
    feat|fix|perf|docs|refactor|test|build) echo "$1" ;;
    *) echo "" ;;
  esac
}

# Scope separators are spelled out rather than using \b, which treats `-` as a
# boundary and would match `foodstuffs-nz` inside a `foodstuffs-nz-cli` scope.
scope_re="^[a-zA-Z]+\(([^)]*[, ])?$project([, ][^)]*)?\)!?:"
commit_re="^([a-zA-Z]+)(\(([^)]*)\))?(!)?:[[:space:]]*(.*)$"

# The main walk happens in a process substitution, where a failing `git log`
# would be swallowed and read as "no commits" -- an empty changelog on a typo
# in the range. Check the range once, up front, where the failure is visible.
if ! git rev-list --no-walk "$range" >/dev/null 2>&1; then
  echo "error: not a range this repository knows: $range" >&2
  exit 1
fi

# Commits that touched the project's directory. Newline-delimited full hashes,
# so a substring can never match.
paths="$(git log --no-merges --format=%H "$range" -- "projects/$project")"

# One entry per selected commit, `group<TAB>rendered line`, in commit order.
entries=()

while IFS= read -r -d '' record; do
  hash="${record%%$'\x1f'*}"
  rest="${record#*$'\x1f'}"
  subject="${rest%%$'\x1f'*}"
  body="${rest#*$'\x1f'}"

  in_path=false
  case $'\n'"$paths"$'\n' in *$'\n'"$hash"$'\n'*) in_path=true ;; esac
  if [[ "$in_path" != true ]] && ! [[ "$subject" =~ $scope_re ]]; then
    continue
  fi

  group="" scope="" bang="" text="$subject"
  if [[ "$subject" =~ $commit_re ]]; then
    type="$(printf '%s' "${BASH_REMATCH[1]}" | tr '[:upper:]' '[:lower:]')"
    group="$(group_for "$type")"
    # Only a type we recognise earns having its prefix stripped; otherwise the
    # subject is prose that happens to contain a colon, and it is kept whole.
    if [[ -n "$group" ]]; then
      scope="${BASH_REMATCH[3]}"
      bang="${BASH_REMATCH[4]}"
      text="${BASH_REMATCH[5]}"
    fi
  fi
  [[ -z "$group" ]] && group=other

  # `feat!:` and a `BREAKING CHANGE:` footer mean the same thing. A breaking
  # commit is listed once, under Breaking changes -- repeating it under its own
  # type would say the same thing twice in a release with three entries.
  if [[ -n "$bang" ]] || printf '%s' "$body" | grep -qE '^BREAKING[ -]CHANGE:'; then
    group=breaking
  fi

  # The scope is almost always the project the notes are for, which every line
  # would then repeat. Drop it, and keep any other scope as a prefix.
  remainder=""
  if [[ -n "$scope" ]]; then
    IFS=',' read -ra parts <<< "$scope"
    for part in "${parts[@]}"; do
      part="${part#"${part%%[![:space:]]*}"}"
      part="${part%"${part##*[![:space:]]}"}"
      [[ -z "$part" || "$part" == "$project" ]] && continue
      remainder="${remainder:+$remainder, }$part"
    done
  fi
  [[ -n "$remainder" ]] && text="**$remainder:** $text"

  entries+=("$group"$'\t'"- $text")
done < <(git log --no-merges -z --format="%H%x1f%s%x1f%B" "$range")

[[ ${#entries[@]} -eq 0 ]] && exit 0

first=true
for group in "${groups[@]}"; do
  key="${group%%|*}"
  heading="${group#*|}"

  printed=false
  for entry in "${entries[@]}"; do
    [[ "${entry%%$'\t'*}" == "$key" ]] || continue
    if [[ "$printed" != true ]]; then
      [[ "$first" == true ]] || echo
      echo "### $heading"
      echo
      printed=true first=false
    fi
    echo "${entry#*$'\t'}"
  done
done
