# claude-playground

A polyglot playground monorepo. Projects here are experiments, one-off tools,
and clones of existing tools adapted to work against something else. Any
language is fair game.

[dispat](https://dispat.dev) is the monorepo tool: it discovers the projects,
runs their scripts, and releases them from the commit history.

## Structure

- `apps/<name>/` — the projects that ship, one directory each, each with its
  own `dispat.yaml`
- `packages/<name>/` — libraries shared between apps, same contract. Code moves
  here when a *second* app needs it, not before
- `templates/<lang>/` — scaffolding for new projects
- `scripts/` — scaffolding (`new-project.sh`), release matrix (`release-matrix.sh`)
- `dispat.yaml` — root config: where projects live, how they are tagged
- `docs/conventions.md` — the full conventions

## Working here

Start a new project with the scaffolder rather than by hand:

```bash
scripts/new-project.sh <c|go|node-ts|python|rust> <kebab-case-name>
scripts/new-project.sh --space packages <template> <kebab-case-name>
```

Then work inside `apps/<name>/` (or `packages/<name>/`):

```bash
dispat run check --since all              # what CI runs, every project
dispat run test  --since all -p <name>    # one project
dispat status                             # what a release would do
dispat preview                            # the notes it would write
```

**`--since all` matters.** Without it, `dispat run` only covers packages the
release window selects — those with commits since their last tag. At a keyboard
you almost always want `--since all`.

## Rules that matter

- **Keep projects self-contained.** A project owns its own dependencies, build
  files, lockfiles and `dispat.yaml`. Do not hoist anything to the repo root —
  no root `package.json`, no cargo workspace, no shared `go.mod`, and no build
  scripts in the root config. Deleting a project directory must fully remove it.
- **Every project needs a `dispat.yaml`** defining as many of `build`, `test`,
  `lint`, `fmt`, `fmt-check`, `run`, `check` and `release-build` as apply. That
  file is the only interface the rest of the repo uses. Omitted scripts are
  skipped, so do not add empty ones just to fill the table.
- **Use the project's own tooling.** A Rust project calls cargo directly; a C
  project calls `make` because it needs real build rules. There is no repo-wide
  build tool to satisfy, so do not add a layer of indirection for symmetry.
- **`check` is the CI contract.** It should be `fmt-check lint build test` minus
  whatever the project does not implement. Keep `build` in there — `test` does
  not necessarily compile every source file.
- **Do not touch other projects** when working on one. They are unrelated by
  design — unless one genuinely depends on another, in which case say so in the
  root `dispat.yaml` under `dependencies` and let dispat order the builds and
  propagate the version bumps. That is the only sanctioned cross-reference, and
  the shared half belongs in `packages/`.
- **No list to update.** dispat discovers projects by their directory under
  `apps/` or `packages/`. Adding a directory is enough.
- **Releases come from commits, not tags.** A `feat(<project>): ...` or
  `fix(<project>): ...` on `main` releases that project; the scope is the
  project's directory name. Do not bump versions by hand and do not push
  release tags — dispat writes the manifest version, the tag and the GitHub
  release. A project that ships binaries implements `release-build` and
  declares its runners under `custom.releasePlatforms`.
- Commit lockfiles. Keep build output in `bin/`, `dist/`, `release/` or
  `target/` — already gitignored.
