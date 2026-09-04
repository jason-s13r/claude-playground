# packages

Libraries the apps share. One directory per package, self-contained the same
way an app is, with its own `dispat.yaml` declaring its scripts.

| Package | What it holds |
| ------- | ------------- |
| [`net-kit`](net-kit) | The process boundary: HTTP with a browser TLS fingerprint, the cookie jar, the credential store, the config file and the paths those live under |
| [`cli-kit`](cli-kit) | Command line presentation: `Out` routing to text or `--json`, tables, prompts, completions, doctor reports |
| [`gsnz-core`](gsnz-core) | The grocery domain: one `Product`, `Cart`, `Order`, `Store`, `Quantity`, and the `Retailer` trait a per-chain adapter implements |
| [`gsnz-ui`](gsnz-ui) | `cli_kit::View` implementations over `gsnz-core` types — product listings, carts, orders, department trees, comparison tables |
| [`fsnz-api`](fsnz-api) | The Foodstuffs edge API (New World and PAK'nSAVE) and the Club Plus login |
| [`wwnz-api`](wwnz-api) | The Woolworths GraphQL API and its Auth0 login flow |
| [`twlnz-api`](twlnz-api) | The Warehouse storefront: listings scraped from HTML, plus the JSON cart, wishlist and stores |
| [`build-kit`](build-kit) | The provenance a `build.rs` stamps into a binary, and the self-update that replaces it from a GitHub release |

Each one documents itself in its own `README.md`, linked above.

They stack in that order. `net-kit` and `cli-kit` know nothing about
groceries; `gsnz-core` knows nothing about HTTP; the two API crates speak their
own vendor-shaped types and convert to the domain in the app, which is what
keeps each of them usable on its own. `twlnz-api` is vendor-shaped too but has
no domain to convert to: nothing above it is a grocery, so its types are the
final ones. Consumers and the edges dispat follows
are the `dependencies` block in the root [`dispat.yaml`](../dispat.yaml).

Each crate exports its own `VERSION`, which is how `gsnz --version` can name
the library versions it was compiled against — they release on separate tags,
so the binary's own number does not say which `fsnz-api` is inside it.

```bash
scripts/new-project.sh --space packages <c|go|node-ts|python|rust> <name>
```

The templates are app-shaped — a hello-world with a `main` and a test — so
scaffolding a library means deleting the entry point afterwards. A library
template gets added when a second library wants one.

## When something belongs here

When a *second project* needs it — another app, or another package. One
consumer means it belongs in that consumer; extracting a library ahead of its
second caller guesses at the interface, and the guess is usually wrong. A
package with one consumer is an app's own module that has been moved further
away.

There is one other reason a directory here is justified: when the boundary
exists to keep a dependency **out**. A presentation library that cannot reach
the network, or a domain library carrying nothing heavier than `serde`, is a
constraint the compiler enforces and a module inside an app cannot. `cli-kit`
and `gsnz-core` were split out for that reason, before either had a second
caller; `net-kit` holds the same kind of line from the other side, forbidding
itself the environment so a caller has to pass values in. A `clippy.toml`
banning `std::env` sits in every library that could have reached for it.

## Depending on one

Apps do not find packages automatically — a dependency is declared twice:

1. In the app's own manifest, however that language does it. For Rust that is a
   path dependency:

   ```toml
   my-lib = { path = "../../packages/my-lib", version = "0.1.0" }
   ```

   **One inline entry per dependency.** dispat rewrites the version of an
   inline entry on release; it does not see the sub-table form, so this

   ```toml
   [build-dependencies.my-lib]     # don't
   path = "../../packages/my-lib"
   version = "0.1.0"
   ```

   is left pinned to a version that no longer exists, and the next `cargo`
   invocation fails with "failed to select a version". The same applies in
   `[dev-dependencies]` and `[build-dependencies]`, where it is tempting
   because the entry has several keys.

   **One `version` per package per manifest.** dispat rewrites one range per
   manifest, so a package declared in both `[dependencies]` and
   `[build-dependencies]` gets the first updated and the second left behind —
   the same "failed to select a version", from the opposite direction. Give the
   second declaration a bare `path` and no `version`; a path dependency only
   needs one to be publishable, and nothing here is published.

2. In the root [`dispat.yaml`](../dispat.yaml), so dispat orders the builds and
   propagates version bumps:

   ```yaml
   dependencies:
     my-app: [my-lib]
   ```

Then a commit that releases the library can carry the bump onward:

```
feat(my-lib)^: add a streaming parser
```

`^` releases the direct consumers too, `^^` the transitive ones. Left off, only
the library releases. dispat rewrites the version in the consumer's manifest to
whatever it just published, so the path stays a path and the number stays
honest.

This is the only sanctioned way one directory may reference another. It does
not make them a workspace: each still owns its manifest, its lockfile and its
own `dispat.yaml`.
