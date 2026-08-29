# claude-playground

A polyglot playground monorepo. Projects here are experiments, one-off tools,
and clones of existing tools adapted to work against something else. Any
language is fair game.

## Structure

- `projects/<name>/` — self-contained projects, one directory each
- `templates/<lang>/` — scaffolding for new projects
- `scripts/` — discovery (`list-projects.sh`), fan-out (`fanout.sh`), scaffolding (`new-project.sh`)
- `docs/conventions.md` — the full conventions

## Working here

Start a new project with the scaffolder rather than by hand:

```bash
make new TEMPLATE=<c|go|node-ts|python|rust> NAME=<kebab-case-name>
```

Then work inside `projects/<name>/`. Run things through the root Makefile:

```bash
make test P=<name>
make run  P=<name> ARGS="..."
make check P=<name>     # what CI runs; run this before committing
make check              # every project
```

## Rules that matter

- **Keep projects self-contained.** A project owns its own dependencies, build
  files and lockfiles. Do not hoist anything to the repo root — no root
  `package.json`, no cargo workspace, no shared `go.mod`. Deleting a project
  directory must fully remove it.
- **Every project needs a `Makefile`** implementing as many of `build`, `test`,
  `lint`, `fmt`, `fmt-check`, `run`, `clean`, `check` as apply. That Makefile is
  the only interface the rest of the repo uses. Omitted targets are skipped by
  the fan-out, so do not add empty ones just to fill the table.
- **`check` is the CI contract.** It should be `fmt-check lint test` minus
  whatever the project does not implement.
- **Do not touch other projects** when working on one. They are unrelated by
  design.
- **No list to update.** CI and the root Makefile discover projects by globbing
  `projects/*/Makefile`. Adding a directory is enough.
- If a project needs a language or toolchain without a template, just create the
  directory and its Makefile by hand; add a template only if it is likely to be
  reused.
- Commit lockfiles. Keep build output in `bin/`, `dist/` or `target/` — already
  gitignored.
