# Conventions

The point of this repo is to make starting a new thing cheap, and to keep the
things that already exist from interfering with each other. Two ideas do most
of the work:

1. **Projects are self-contained.** A project owns its dependencies, its build
   files and its lockfiles. Nothing is hoisted to the root — no shared
   `package.json`, no cargo workspace, no root `go.mod`. Deleting
   `projects/<name>/` removes the project completely.
2. **Projects share one interface.** Whatever the language, the project is
   driven through `make`. That is the whole contract between a project and the
   rest of the repo.

## Adding a project

```bash
make new TEMPLATE=go NAME=port-scan
```

This copies `templates/go/` to `projects/port-scan/`, substituting two
placeholders:

- `__NAME__` → the project name as given (`port-scan`)
- `__IDENT__` → a language-safe identifier (`port_scan`), also substituted in
  file and directory names

Names are lowercase kebab-case. If no template fits, create the directory by
hand — all a project needs to join the repo is a `Makefile`.

## Adding a language

Templates are added on demand. There is one each for `c`, `go`, `node-ts`,
`python` and `rust`; a language outside that set is not blocked, it just has no
starting point yet.

When a project needs a language with no template, write the project directly in
`projects/<name>/` with a hand-written `Makefile` implementing the contract
above. That is enough for the root runner and CI to pick it up. Promote the
setup to `templates/<lang>/` only when a second project in that language shows
up and there is something worth copying — a template that has never been used
twice is a guess, not a convention.

A template is a working hello-world with a test and a Makefile: run
`make new TEMPLATE=<lang> NAME=tmp && make check P=tmp` before committing one,
so it is known to build rather than assumed to.

## Project Makefiles

Implement the targets in the table in the [README](../README.md). Guidance:

- `check` is the CI entry point. It should be `fmt-check lint build test`, minus
  any of those the project does not implement. Include `build` even when `test`
  appears to compile everything: a C test binary links only the units it needs,
  so without it a `main.c` that does not compile still passes.
- `run` should accept `ARGS`, so `make run P=x ARGS="--verbose"` works.
- Prefer failing loudly on real problems, but let an *optional* formatter or
  linter that is not installed print a note and pass. The templates do this for
  `clippy`, `ruff`, `prettier` and `clang-format`, so a fresh clone can run
  `make check` without installing five toolchains.
- Keep artifacts inside the project, in `bin/`, `dist/` or `target/` — these
  are already ignored at the root.

## Dependencies

Commit lockfiles (`Cargo.lock`, `package-lock.json`, `go.sum`, `uv.lock`).
`.gitattributes` marks them generated so they stay out of diffs and out of the
repo's language statistics.

## Layout inside a project

There is no repo-wide rule; follow whatever is idiomatic for the language. The
templates show a reasonable default for each. Anything a project wants to
explain about itself goes in its own `README.md`.

## Scratch work

`scratch/` and `tmp/` are ignored at the root. Use them for throwaway files
rather than leaving them loose in a project.

## Releasing

Tag `<project>/vX.Y.Z` and push it. The release workflow validates the tag,
runs `make check`, runs `make dist`, and publishes the contents of the
project's `release/` directory.

A project that should be releasable implements `dist`:

- Put artifacts in `release/`, not `dist/` — `dist/` is build output for tsc
  and setuptools, and packing into it would feed artifacts back into the next
  build.
- Name them `<project>-<version>-<os>-<arch>` where the project controls
  naming. `VERSION` is exported by the root Makefile and defaults to
  `0.0.0-dev` for local builds.
- `dist` is optional. The fan-out skips a project that has no `dist` target,
  but a release for such a project fails on the empty artifact directory
  rather than publishing nothing.

If the language embeds a version from a manifest, leave it that way and let the
workflow enforce that the tag matches. Do not have `dist` rewrite the manifest:
it makes local builds mutate tracked files.
