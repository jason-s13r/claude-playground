#!/usr/bin/env bash
# Scaffold a new project from a template in templates/.
#
# Usage:
#   scripts/new-project.sh <template> <name>
#   scripts/new-project.sh --list
#
# Templates are ordinary project trees with two placeholders substituted on
# copy: __NAME__ (the project name as given, e.g. "my-tool") and __IDENT__
# (a language-safe identifier, e.g. "my_tool"). __IDENT__ is also
# substituted in file and directory names.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
templates_dir="$repo_root/templates"
projects_dir="$repo_root/projects"

list_templates() {
  for d in "$templates_dir"/*/; do
    [[ -d "$d" ]] || continue
    basename "$d"
  done
}

if [[ "${1:-}" == "--list" ]]; then
  list_templates
  exit 0
fi

template="${1:-}"
name="${2:-}"

if [[ -z "$template" || -z "$name" ]]; then
  cat >&2 <<USAGE
usage: $(basename "$0") <template> <name>

available templates:
$(list_templates | sed 's/^/  /')
USAGE
  exit 2
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

dest="$projects_dir/$name"
if [[ -e "$dest" ]]; then
  echo "error: projects/$name already exists" >&2
  exit 1
fi

ident="${name//-/_}"

mkdir -p "$projects_dir"
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

echo "created projects/$name from the '$template' template"
echo
echo "next:"
echo "  dispat run test -p $name"
echo "  dispat run check -p $name"
echo
echo "note: dispat only runs a package the release window selects, so add"
echo "      --since all until $name has a commit of its own."
