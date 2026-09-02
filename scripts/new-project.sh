#!/usr/bin/env bash
# Scaffold a new project from a template in templates/.
#
# Usage:
#   scripts/new-project.sh [--space apps|packages] <template> <name>
#   scripts/new-project.sh --list
#
# The space is the directory it lands in: `apps` for something that ships,
# `packages` for a library the apps share. Both are dispat spaces and are
# discovered the same way.
#
# Templates are ordinary project trees with two placeholders substituted on
# copy: __NAME__ (the project name as given, e.g. "my-tool") and __IDENT__
# (a language-safe identifier, e.g. "my_tool"). __IDENT__ is also
# substituted in file and directory names.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
templates_dir="$repo_root/templates"

list_templates() {
  for d in "$templates_dir"/*/; do
    [[ -d "$d" ]] || continue
    basename "$d"
  done
}

usage() {
  cat >&2 <<USAGE
usage: $(basename "$0") [--space apps|packages] <template> <name>

available templates:
$(list_templates | sed 's/^/  /')
USAGE
  exit 2
}

if [[ "${1:-}" == "--list" ]]; then
  list_templates
  exit 0
fi

space="apps"
args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --space)
      space="${2:-}"
      shift 2 || usage
      ;;
    --space=*)
      space="${1#--space=}"
      shift
      ;;
    -*)
      echo "error: unknown flag: $1" >&2
      usage
      ;;
    *)
      args+=("$1")
      shift
      ;;
  esac
done

template="${args[0]:-}"
name="${args[1]:-}"

[[ -n "$template" && -n "$name" ]] || usage

# The spaces are declared in the root dispat.yaml; this list is the same one.
if [[ "$space" != "apps" && "$space" != "packages" ]]; then
  echo "error: space must be 'apps' or 'packages' (got: $space)" >&2
  exit 1
fi

if [[ ! -d "$templates_dir/$template" ]]; then
  echo "error: no such template: $template" >&2
  echo "available: $(list_templates | tr '\n' ' ')" >&2
  exit 1
fi

if [[ ! "$name" =~ ^[a-z0-9][a-z0-9-]*$ ]]; then
  echo "error: project name must be lowercase kebab-case (got: $name)" >&2
  exit 1
fi

space_dir="$repo_root/$space"
dest="$space_dir/$name"
if [[ -e "$dest" ]]; then
  echo "error: $space/$name already exists" >&2
  exit 1
fi

ident="${name//-/_}"

mkdir -p "$space_dir"
cp -R "$templates_dir/$template" "$dest"

# Rename paths containing __IDENT__, deepest first so parents stay valid.
while IFS= read -r path; do
  [[ -n "$path" ]] || continue
  mv "$path" "${path//__IDENT__/$ident}"
done < <(find "$dest" -depth -name '*__IDENT__*')

# Substitute placeholders in file contents.
while IFS= read -r -d '' file; do
  sed -i.bak -e "s/__NAME__/$name/g" -e "s/__IDENT__/$ident/g" "$file"
  rm -f "$file.bak"
done < <(find "$dest" -type f -print0)

echo "created $space/$name from the '$template' template"
echo
echo "next:"
echo "  dispat run test -p $name"
echo "  dispat run check -p $name"
echo
echo "note: dispat only runs a package the release window selects, so add"
echo "      --since all until $name has a commit of its own."
