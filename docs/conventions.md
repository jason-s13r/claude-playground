# Conventions

The point of this repo is to make starting a new thing cheap, and to keep the
things that already exist from interfering with each other. Two ideas do most
of the work:

1. **Projects are self-contained.** A project owns its dependencies, its build
   files, its lockfiles and its own `dispat.yaml`. Nothing is hoisted to the
   root — no shared `package.json`, no cargo workspace, no root `go.mod`, no
   build scripts in the root config. Deleting its directory removes it
   completely.
2. **Projects declare themselves.** Whatever the language, a project says what
   can be done to it in its own `dispat.yaml`. That is the whole contract
   between a project and the rest of the repo.

[dispat](https://dispat.dev) reads those declarations. It discovers the
projects, runs their scripts, works out from the commit history which ones
changed, and releases them.

## The two spaces

Projects live in one of two directories, and dispat treats them identically —
same contract, same discovery, same release mechanism. The difference is who
they are for:

- **`apps/`** — the things that ship. A CLI, a TUI, a web app. Almost
  everything starts here.
- **`packages/`** — libraries the projects share. Something belongs here when a
  *second project* needs it — an app or another package. Extracting a library
  for one caller guesses at the interface, and the guess is usually wrong; a
  package with one consumer is an app's own module that has been moved further
  away. The one other justification is a boundary that exists to keep a
  dependency *out*: a presentation library that cannot reach the network, or a
  domain library with nothing but `serde` in it, earn their own directory even
  before the second caller arrives.

Both are declared in the root [`dispat.yaml`](../dispat.yaml) as spaces, each
named for its directory. Nothing else distinguishes them.

## Adding a project

```bash
scripts/new-project.sh go my-tool                   # into apps/
scripts/new-project.sh --space packages go my-lib   # into packages/
```

This copies `templates/go/` to `apps/my-tool/`, substituting two
placeholders:

- `__NAME__` → the project name as given (`my-tool`)
- `__IDENT__` → a language-safe identifier (`my_tool`), also substituted in
  file and directory names

Names are lowercase kebab-case. If no template fits, create the directory by
hand — all a project needs to join the repo is a `dispat.yaml`.

## Adding a language

Templates are added on demand. There is one each for `c`, `go`, `node-ts`,
`python` and `rust`; a language outside that set is not blocked, it just has no
starting point yet.

When a project needs a language with no template, write the project directly in
`apps/<name>/` with a hand-written `dispat.yaml`. That is enough for dispat
and CI to pick it up. Promote the setup to `templates/<lang>/` only when a
second project in that language shows up and there is something worth copying —
a template that has never been used twice is a guess, not a convention.

A template is a working hello-world with a test and a `dispat.yaml`: run
`scripts/new-project.sh <lang> tmp && dispat run check --since all -p tmp`
before committing one, so it is known to build rather than assumed to.

## Project scripts

Implement the scripts in the table in the [README](../README.md). Guidance:

- **Call the project's own tooling.** cargo, npm, go, python. A C project calls
  `make` because it needs real build rules; that is the project choosing its
  tool, not the repo imposing one. Do not add a wrapper for symmetry — a layer
  that only forwards to cargo is a layer that can break.
- `check` is the CI entry point. It should be `fmt-check lint build test`, minus
  any of those the project does not implement. Include `build` even when `test`
  appears to compile everything: a C test binary links only the units it needs,
  so without it a `main.c` that does not compile still passes.
- A script may be a list of commands, which run in order. That is how `check`
  is usually written.
- Prefer failing loudly on real problems, but let an *optional* formatter or
  linter that is not installed print a note and pass. The templates do this for
  `clippy`, `ruff`, `prettier` and `clang-format`, so a fresh clone can run
  `check` without installing five toolchains.
- Keep artifacts inside the project, in `bin/`, `dist/`, `release/` or
  `target/` — these are already ignored at the root.

Scripts run with the project directory as the working directory, and with the
release's identity in the environment: `DISPAT_PACKAGE`, `DISPAT_NEW_VERSION`
and the rest. `$PWD` is the project folder, which is the easy way to make a
path absolute.

## The release window

`dispat run <script>` covers the packages the *release window* selects — those
with commits since their own last tag. That is what a release wants and rarely
what a person at a keyboard wants, so day to day:

```bash
dispat run check --since all          # every project
dispat run check --since all -p foo   # one project
```

`--since` also takes a revision (`origin/main`, `HEAD~3`, a tag), which selects
the packages the commits since then address.

## Dependencies between projects

Projects are unrelated by default. When one genuinely depends on another — a
library in `packages/` and the app in front of it — say so in the root
`dispat.yaml`:

```yaml
dependencies:
  my-cli: [my-lib]
```

The app's own manifest declares it too, however that language does it; for Rust
that is a path dependency carrying a version.

dispat then builds them in order, and a commit that bumps the library
propagates to its consumers when you ask it to:

```
feat(my-lib)^: add a streaming parser
```

The `^` reaches direct consumers; `^^` reaches all of them transitively. Left
off, only `my-lib` releases. dispat also rewrites the dependency range in the
consumer's manifest to the version just released, so a `path` dependency keeps
its path and gains the right version.

This is the one case where a project may reference another. It does not make
them a workspace: each still owns its own manifest and lockfile.

## Layout inside a project

There is no repo-wide rule; follow whatever is idiomatic for the language. The
templates show a reasonable default for each. Anything a project wants to
explain about itself goes in its own `README.md`.

## Scratch work

`scratch/` and `tmp/` are ignored at the root. Use them for throwaway files
rather than leaving them loose in a project.

## Commit messages

Commits are the release mechanism, so the subject line matters. Scope a commit
with the project's **directory name** — the same name that appears in its tags:

```
fix(foodstuffs-nz-cli): stop `check` swallowing lint failures
```

The type decides the bump: `feat` is a minor, `fix` and `perf` are patches, and
a `!` before the colon or a `BREAKING CHANGE:` footer makes it a major. Other
types (`chore`, `docs`, `refactor`, `test`, `ci`, `style`) release nothing on
their own but still appear in the notes.

A commit with no scope is attributed by the files it touched, so repo-wide work
on `scripts/` or `.github/` belongs to no project and releases nothing. dispat
says so with a `W131` warning; that is it agreeing with you, not a problem to
fix. An explicit scope is the only way a change that belongs to a project
without living in its directory reaches that project's notes.

A binary's name (`fsnz`) is not a scope; nothing matches on it. A commit
touching two projects can name both — `fix(a,b): ...` — but prefer splitting it,
since one subject rarely describes both well.

Run `dispat preview` to see exactly what the notes would say before pushing.

## Releasing

Push to `main`. dispat reads the commits since each project's last tag, works
out which projects changed and what their next versions are, and releases those.
Nothing releasable in the push means nothing happens.

```bash
dispat status     # what would be released, and at what version
dispat preview    # the notes that would be written
```

Do not bump versions by hand and do not push release tags. dispat writes the
new version into the project's manifest, makes the release commit, tags it
`<project>/vX.Y.Z`, and creates the GitHub release. The number in the manifest,
the number in the tag and the number baked into the binary cannot disagree,
because one thing writes all three.

A project that ships binaries implements `release-build`:

- Put artifacts in `release/`, not `dist/` — `dist/` is build output for tsc
  and setuptools, and packing into it would feed artifacts back into the next
  build.
- Name them `<project>-<version>-<os>-<arch>`. Every platform's artifacts land
  in one directory, so the platform in the name is what keeps two runners from
  overwriting each other.
- Export the finished list for upload, which is also what opts the project into
  a GitHub release at all:

  ```sh
  echo "DISPAT_EXPORT_GITHUB=$(ls -d "$PWD/release/"* | tr '\n' ' ')" >> "$DISPAT_OUTPUT"
  ```

- Write `SHA256SUMS` over every artifact, once. A partial one breaks
  `fsnz update`-style verification on the platforms it missed.

There is no cross-compiling: a platform is built on its own runner or not at
all. A project shipping more than one declares them in its own `dispat.yaml`:

```yaml
custom:
  releasePlatforms: [ubuntu-latest, macos-14]
```

`scripts/release-matrix.sh` turns that plus dispat's plan into the build matrix
the release workflow fans out over. Declaring nothing builds on `ubuntu-latest`
alone, so nothing changes for projects that ship source, or ship nothing.
