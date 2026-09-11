# shopping-cli-tools

Command line tools for shopping at New Zealand retailers — and the libraries
they are made of.

Five binaries, all unofficial, all reverse-engineered from what the retailers'
own websites call from a browser:

| Binary | App | Covers |
| --- | --- | --- |
| `gsnz` | [`grocery-nz-cli`](apps/grocery-nz-cli) | New World, PAK'nSAVE and Woolworths NZ side by side — `gsnz compare "2l milk"` |
| `fsnz` | [`foodstuffs-nz-cli`](apps/foodstuffs-nz-cli) | New World and PAK'nSAVE, one Foodstuffs client driving both |
| `wwnz` | [`woolworths-nz-cli`](apps/woolworths-nz-cli) | Woolworths NZ |
| `kmart` | [`kmart-cli`](apps/kmart-cli) | Kmart, Australia and New Zealand off one backend |
| `twlnz` | [`the-warehouse-nz-cli`](apps/the-warehouse-nz-cli) | The Warehouse NZ |

`gsnz` is the whole point and the other two supermarkets are its single-chain
slices: the question worth answering is *is it cheaper at the other one?* The
Warehouse and Kmart are general merchandise, not groceries, so they share none
of the grocery domain and all of the plumbing.

None of the retailers offer a public API. Everything here is built by reading
the sites' own traffic, and it breaks when they change something.

## Structure

`apps/` ship; `packages/` are the libraries they are built from, and both
release the same way. Code moves to `packages/` when a *second* app needs it,
not before.

```console
$ dispat status
```

dispat discovers projects by their directory, so there is no list to update:
adding a directory under `apps/` or `packages/` is the entire registration.

## The libraries

The apps are thin front ends. The parts worth reading are the nine crates in
[`packages/`](packages):

| Crate | What it holds |
| --- | --- |
| [`net-kit`](packages/net-kit) | the process boundary: browser-fingerprinted HTTP, a persisted cookie jar, the OS credential store, config paths |
| [`cli-kit`](packages/cli-kit) | presentation with no domain: tables, `--json`, prompts, `doctor`, completions |
| [`gsnz-core`](packages/gsnz-core) | the grocery domain — one `Product`, `Cart`, `Order`, `Store`, and the `Retailer` trait |
| [`gsnz-ui`](packages/gsnz-ui) | the grocery renderers, every one a `cli_kit::View` over a `gsnz-core` type |
| [`fsnz-api`](packages/fsnz-api) | the Foodstuffs edge API and the Club Plus login, in Foodstuffs' own vocabulary |
| [`wwnz-api`](packages/wwnz-api) | the Woolworths GraphQL API and its Auth0 flow |
| [`kmart-api`](packages/kmart-api) | Kmart's catalogue, stock, cart and login, both countries |
| [`twlnz-api`](packages/twlnz-api) | The Warehouse's Salesforce storefront — mostly HTML, and the one crate that parses it |
| [`build-kit`](packages/build-kit) | the build stamp and the `update` command that swaps the binary for a newer release |

Two rules hold the shape together:

- **The API crates are vendor-shaped on purpose.** `fsnz-api` speaks Foodstuffs'
  vocabulary and depends on no shared domain crate; converting to `gsnz-core`
  is the app's job, the adapter lives in the app. That keeps a Foodstuffs
  quirk from leaking into a type Woolworths also has to fit, and it is why
  `twlnz-api` and `kmart-api` share none of `gsnz-core` at all: general
  merchandise has nowhere to put a colour or size axis in a `SaleUnit`.
- **The libraries read no environment.** `net-kit` and `cli-kit` take every
  setting as a value — `clippy.toml` fails the build over `std::env::var` —
  so an app reads its environment once, at the top, and passes the results
  down. A variable name read inside a library is a variable name every
  consumer is stuck with.

## Why `wreq` and not `reqwest`

These storefronts sit behind Cloudflare and Akamai, which fingerprint the TLS
handshake and HTTP/2 settings rather than the headers. Every `reqwest` TLS
backend is scored as a bot and answered with a bare 400 or a challenge page,
with nothing in it that says why. `net-kit` builds on `wreq`, which presents a
real browser's fingerprint, and the same requests are answered normally.

The same idea explains the rest of the credential handling: cookies live in
the OS credential store rather than a plaintext file, and Kmart's Akamai check
— which genuinely cannot be passed by a command line tool — is met by
importing browser cookies or driving a real browser for the one login step
that refuses anything else.

## Install

Each app builds and installs on its own; nothing hoists to the repo root:

```bash
cd apps/grocery-nz-cli
cargo install --path .     # or apps/foodstuffs-nz-cli, or any of the five
```

Published builds live on
[releases](https://github.com/jason-s13r/shopping-cli-tools/releases), tagged
`<app>/vX.Y.Z` — one release per app, per library, never one for the repo. Each
release carries `linux-x86_64` and `darwin-arm64` binaries and a `SHA256SUMS`
covering them. Once installed, a binary replaces itself:

```console
$ gsnz update --check     # is there a newer one, and what changed in it?
$ gsnz update             # download it and swap it in
```

On any other platform `update` says what the release does have and leaves the
binary alone; build from source instead.

## Releases come from commits

dispat reads the conventional commits since each project's last tag and
releases only what changed. A `feat(<project>): ...` or `fix(<project>): ...`
on `main` releases that project; the scope is the directory name.

```yaml
# dispat.yaml — the only sanctioned cross-reference
dependencies:
  foodstuffs-nz-cli: [gsnz-core, gsnz-ui, cli-kit, net-kit, fsnz-api, build-kit]
```

The root `dispat.yaml` declares the dependency graph so dispat can order the
builds and propagate a library's bump into every app that depends on it —
without it, releasing `net-kit` would leave an app pinned to a version that no
longer exists. Nothing else crosses a project boundary: no root `package.json`,
no cargo workspace, and deleting a project directory must fully remove it.

## Working here

```bash
dispat run check --since all              # what CI runs, every project
dispat run test  --since all -p cli-kit    # one project
dispat status                             # what a release would do
dispat preview                            # the notes it would write
```

**`--since all` matters.** Without it, `dispat run` only covers packages the
release window selects — those with commits since their last tag. At a keyboard
you almost always want `--since all`.

Each project owns its own dependencies, build files, lockfiles and
`dispat.yaml` defining as many of `build`, `test`, `lint`, `fmt`, `fmt-check`,
`run`, `check` and `release-build` as apply. That file is the only interface
the rest of the repo uses. `check` is the CI contract — `fmt-check lint build
test` minus whatever the project does not implement — and it is what
[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on every push.

A project shipping binaries implements `release-build` and declares its
runners under `custom.releasePlatforms`; there is no cross-compiling, so each
platform is built on its own runner.

## Disclaimer

Not affiliated with Foodstuffs New Zealand, New World, PAK'nSAVE, Woolworths
New Zealand, The Warehouse, or Kmart. There are no public APIs. These tools
call the same undocumented endpoints the retailers' own frontends call, and can
break whenever they change something. Use at your own risk.

## License

[Unlicense](LICENSE) — public domain. Do anything you like with it.