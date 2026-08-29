# claude-playground

A polyglot monorepo for one-off tools, experiments, clones and rewrites.
Anything goes: C, C++, Rust, Go, Node/TypeScript, Python — CLIs, TUIs, web
apps, whatever the current idea needs.

The only rule is that each project stays **self-contained** and exposes the
**same small `make` interface**, so the repo can build and test everything
without knowing what anything is written in.

## Layout

```
projects/     one directory per project, each with its own Makefile
templates/    starting points for new projects, one per language
scripts/      repo tooling (project discovery, scaffolding, fan-out)
docs/         conventions and notes
Makefile      root task runner that fans out to projects
```

## Quick start

```bash
make                              # help, plus the current project list
make new TEMPLATE=rust NAME=my-tool
make run  P=my-tool ARGS="--help"
make test P=my-tool
make check                        # fmt-check + lint + test, every project
```

Available templates: `c`, `go`, `node-ts`, `python`, `rust`.
Nothing forces you to use one — a project only needs a `Makefile`.

## The project contract

Every directory under `projects/` with a `Makefile` is a project. The root
runner fans these targets out to all of them (or to one, with `P=<project>`):

| Target      | Meaning                                            |
| ----------- | -------------------------------------------------- |
| `build`     | Produce whatever the project builds                |
| `test`      | Run the tests                                      |
| `lint`      | Static analysis                                    |
| `fmt`       | Format sources in place                            |
| `fmt-check` | Verify formatting, change nothing                  |
| `run`       | Run the thing; takes `ARGS="..."`                  |
| `clean`     | Remove build artifacts                             |
| `check`     | `fmt-check` + `lint` + `test` — what CI runs        |

A project may omit any target it has no use for; the fan-out skips it instead
of failing. Only `check` really matters, since that is what CI calls.

## CI

`.github/workflows/ci.yml` discovers projects the same way the Makefile does
and runs `make check` for each one as a separate job, so a broken experiment
in one language never blocks the others. Adding a project to `projects/` is
enough to put it in CI — there is no list to maintain.

## Conventions

See [`docs/conventions.md`](docs/conventions.md) for how projects are expected
to be laid out, and [`CLAUDE.md`](CLAUDE.md) for the version of that aimed at
Claude Code.

## License

Public domain (Unlicense). See [`LICENSE`](LICENSE).
