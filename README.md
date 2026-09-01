# claude-playground

A polyglot monorepo for one-off tools, experiments, clones and rewrites.
Anything goes: C, C++, Rust, Go, Node/TypeScript, Python — CLIs, TUIs, web
apps, whatever the current idea needs.

The only rule is that each project stays **self-contained**. A project owns its
dependencies, its build files, its lockfiles, and its own `dispat.json` saying
how to build and test it. Deleting `projects/<name>/` removes the project
completely.

[dispat](https://dispat.dev) is the tool that ties them together. It discovers
the projects, runs their scripts, works out which ones changed from the commit
history, and releases them.

## What is here

| Project | What it is |
| ------- | ---------- |
| [`foodstuffs-nz-cli`](projects/foodstuffs-nz-cli) | `fsnz` — search New World and PAK'nSAVE from the terminal, and price one query at both |
| [`woolworths-nz-cli`](projects/woolworths-nz-cli) | `wwnz` — the same for Woolworths NZ, against their GraphQL API |

That table is for people. dispat and CI discover the projects themselves, so
adding one means adding a directory and nothing else.

## Layout

```
projects/     one directory per project, each with its own dispat.json
templates/    starting points for new projects, one per language
scripts/      repo tooling (scaffolding, the release build matrix)
docs/         conventions and notes
dispat.json   the root config: where projects live, how they are tagged
```

## Quick start

```bash
dispat run check --since all             # everything, every project
dispat run test  --since all -p my-tool  # one project
dispat status                            # what a release would do right now
dispat preview                           # the notes it would write
scripts/new-project.sh rust my-tool      # scaffold a project
```

`--since all` is the flag you will type most. Without it, `dispat run` only
covers packages the *release window* selects — the ones with commits since
their last tag — which is what you want in a release and rarely what you want
at a keyboard.

Available templates: `c`, `go`, `node-ts`, `python`, `rust`. Nothing forces you
to use one — a project only needs a `dispat.json`, so a language without a
template is not blocked. New templates get added when a project actually needs
one.

## The project contract

Every directory under `projects/` is a project, and its `dispat.json` says what
can be done to it. By convention those scripts are:

| Script          | Meaning                                                |
| --------------- | ------------------------------------------------------ |
| `build`         | Produce whatever the project builds                    |
| `test`          | Run the tests                                          |
| `lint`          | Static analysis                                        |
| `fmt`           | Format sources in place                                |
| `fmt-check`     | Verify formatting, change nothing                      |
| `run`           | Run the thing                                          |
| `check`         | `fmt-check` + `lint` + `build` + `test` — what CI runs  |
| `release-build` | Build release artifacts and name them for upload       |

A project may omit any script it has no use for; dispat skips a package that
does not define the one being run. Only `check` really matters, since that is
what CI calls.

What those scripts *are* is the project's business. A Rust project calls
cargo, a Node one calls npm, and a C project calls `make` because C genuinely
needs the build rules. There is no repo-wide build tool to satisfy.

## Releases

Releases are driven by [conventional commits](docs/conventions.md#commit-messages),
not by pushing a tag. Push to `main` and dispat reads the commits since each
project's last tag, decides which projects changed and how far to bump them,
and releases those:

```
feat(my-tool): add a --json flag     →  my-tool/v0.2.0
fix(my-tool): stop eating the error  →  my-tool/v0.1.1
```

A push with nothing releasable in it does nothing. `dispat status` and
`dispat preview` show the plan and the notes without touching anything.

For a project that ships binaries, the release is the delivery: the tag, the
GitHub release, its notes taken from the same commits, and the artifacts with a
`SHA256SUMS` covering all of them. A project declares the runners it needs in
its own `dispat.json`, since there is no cross-compiling:

```json
"custom": { "releasePlatforms": ["ubuntu-latest", "macos-14"] }
```

Declaring nothing builds on `ubuntu-latest` alone.

Versions are not hand-maintained. dispat writes the new version into the
project's manifest (`Cargo.toml`, `package.json`, `pyproject.toml`) as part of
the release, so the number in the manifest, the number in the tag and the
number baked into the binary cannot disagree.

## CI

`.github/workflows/ci.yml` runs `dispat run check --since all`. Adding a
project to `projects/` is enough to put it in CI — there is no list to
maintain.

## Conventions

See [`docs/conventions.md`](docs/conventions.md) for how projects are expected
to be laid out, and [`CLAUDE.md`](CLAUDE.md) for the version of that aimed at
Claude Code.

## License

Public domain (Unlicense). See [`LICENSE`](LICENSE).
