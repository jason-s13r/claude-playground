# claude-playground

A polyglot monorepo for one-off tools, experiments, clones and rewrites.
Anything goes: C, C++, Rust, Go, Node/TypeScript, Python — CLIs, TUIs, web
apps, whatever the current idea needs.

The only rule is that each project stays **self-contained**. A project owns its
dependencies, its build files, its lockfiles, and its own `dispat.yaml` saying
how to build and test it. Deleting its directory removes it completely.

[dispat](https://dispat.dev) is the tool that ties them together. It discovers
the projects, runs their scripts, works out which ones changed from the commit
history, and releases them.

## What is here

Two kinds of directory. [`apps/`](apps) holds the things that ship;
[`packages/`](packages) holds the libraries they are built from.

| App | Binary | What it is |
| --- | ------ | ---------- |
| [`grocery-nz-cli`](apps/grocery-nz-cli) | `gsnz` | New World, PAK'nSAVE and Woolworths NZ from one command line, with `compare` pricing a query at all three |
| [`foodstuffs-nz-cli`](apps/foodstuffs-nz-cli) | `fsnz` | The Foodstuffs half on its own: New World and PAK'nSAVE |
| [`woolworths-nz-cli`](apps/woolworths-nz-cli) | `wwnz` | Woolworths NZ on its own, against their GraphQL API |

| Package | What it holds |
| ------- | ------------- |
| [`net-kit`](packages/net-kit) | The process boundary: HTTP that is not scored as a bot, cookies, credentials, and the paths those live under |
| [`cli-kit`](packages/cli-kit) | Command line presentation: output routing, `--json`, tables, prompts, doctor reports — and no domain types |
| [`gsnz-core`](packages/gsnz-core) | The grocery domain: one `Product`, `Cart`, `Order`, `Store`, and the `Retailer` trait a vendor adapter implements. No I/O |
| [`gsnz-ui`](packages/gsnz-ui) | `cli-kit` views over `gsnz-core` types — listings, carts, orders, comparison tables |
| [`fsnz-api`](packages/fsnz-api) | The Foodstuffs edge API and the Club Plus login, in its own vendor-shaped types |
| [`wwnz-api`](packages/wwnz-api) | The Woolworths GraphQL API and its Auth0 login flow, likewise |
| [`build-kit`](packages/build-kit) | The provenance a binary stamps into itself at build time, and the self-update that replaces it |

`gsnz` is built on all seven; `fsnz` and `wwnz` are the two single-chain slices
of it, each dropping the API crate it does not speak. Nothing under `apps/`
carries its own HTTP client, credential store or domain types any more.

Those tables are for people. dispat and CI discover the projects themselves, so
adding one means adding a directory and nothing else.

## Layout

```
apps/         one directory per app, each with its own dispat.yaml
packages/     libraries shared between apps, under the same contract
templates/    starting points for new projects, one per language
scripts/      repo tooling (scaffolding, the release build matrix)
docs/         conventions and notes
dispat.yaml   the root config: where projects live, how they are tagged
```

## Quick start

```bash
dispat run check --since all             # everything, every project
dispat run test  --since all -p my-tool  # one project
dispat status                            # what a release would do right now
dispat preview                           # the notes it would write
scripts/new-project.sh rust my-tool      # scaffold an app
```

`--since all` is the flag you will type most. Without it, `dispat run` only
covers packages the *release window* selects — the ones with commits since
their last tag — which is what you want in a release and rarely what you want
at a keyboard.

Available templates: `c`, `go`, `node-ts`, `python`, `rust`. Nothing forces you
to use one — a project only needs a `dispat.yaml`, so a language without a
template is not blocked. New templates get added when a project actually needs
one.

## The project contract

Every directory under `apps/` or `packages/` is a project, and its
`dispat.yaml` says what can be done to it. By convention those scripts are:

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
its own `dispat.yaml`, since there is no cross-compiling:

```yaml
custom:
  releasePlatforms: [ubuntu-latest, macos-14]
```

Declaring nothing builds on `ubuntu-latest` alone.

Versions are not hand-maintained. dispat writes the new version into the
project's manifest (`Cargo.toml`, `package.json`, `pyproject.toml`) as part of
the release, so the number in the manifest, the number in the tag and the
number baked into the binary cannot disagree.

Libraries release on their own tags, so a bump can be carried onward to the
apps in front of them — `feat(net-kit)^: ...` releases the direct consumers
too, `^^` the transitive ones, and dispat rewrites the version in each
consumer's manifest to what it just published. The edges it follows are the
`dependencies` block in the root [`dispat.yaml`](dispat.yaml); see
[`packages/README.md`](packages) for how a dependency is declared.

## CI

`.github/workflows/ci.yml` runs `dispat run check --since all`. Adding a
directory under `apps/` or `packages/` is enough to put it in CI — there is no
list to maintain.

## Conventions

See [`docs/conventions.md`](docs/conventions.md) for how projects are expected
to be laid out, and [`CLAUDE.md`](CLAUDE.md) for the version of that aimed at
Claude Code.

## License

Public domain (Unlicense). See [`LICENSE`](LICENSE).
