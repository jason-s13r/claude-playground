# woolworths-nz-cli

Search and shop Woolworths New Zealand from the terminal — the other half of
the supermarket duopoly [`foodstuffs-nz-cli`](../foodstuffs-nz-cli) covers. It
replaces [`woolies-nz-cli`](https://github.com/mcinteerj/woolies-nz-cli).

> **Not affiliated with Woolworths New Zealand.** There is no public API. This
> calls the same undocumented GraphQL endpoint their website calls from the
> browser, and can break whenever they change something. Use at your own risk.

## How it is built

`wwnz` is a thin front end. The parts worth reading are in `packages/`:

| Crate | What it holds |
| --- | --- |
| [`gsnz-core`](../../packages/gsnz-core) | the grocery domain: one `Product`, `Cart`, `Order`, `Store`, and the `Retailer` trait |
| [`wwnz-api`](../../packages/wwnz-api) | the Woolworths GraphQL API and its Auth0 login flow, in its own vendor-shaped types |
| [`cli-kit`](../../packages/cli-kit) + [`gsnz-ui`](../../packages/gsnz-ui) | tables, `--json`, and the grocery renderers |
| [`net-kit`](../../packages/net-kit) | the browser-fingerprinted HTTP client, the cookie jar and the credential store |
| [`build-kit`](../../packages/build-kit) | the build stamp and `wwnz update` |

What is left in `src/` is the part that is genuinely this program: reading the
environment once, resolving flags against config, adapting `wwnz-api` types to
`gsnz-core`, and turning a failure into an exit code.

[`grocery-nz-cli`](../grocery-nz-cli) (`gsnz`) is the same architecture with
both Foodstuffs banners added alongside; `wwnz` is the Woolworths-only slice,
and [`fsnz`](../foodstuffs-nz-cli) is the other one.

## Install

```bash
cargo build                        # from this directory
cargo install --path .             # or install the `wwnz` binary
```

Or take a published build from
[releases](https://github.com/jason-s13r/claude-playground/releases), which are
tagged `woolworths-nz-cli/vX.Y.Z`. Once you have a binary it can replace itself:

```bash
wwnz update --check     # is there a newer one?
wwnz update             # download it and swap it in
```

Releases publish `linux-x86_64` and `darwin-arm64` binaries. On anything else
`wwnz update` says what the release does have and leaves the binary alone;
build from source instead.

## Quick start

```bash
wwnz stores whangarei                   # find a store
wwnz store set "Regent Woolworths"      # bind it
wwnz search milk
wwnz search milk --size 2L --limit 5
wwnz specials --limit 40
wwnz departments                        # what browse can select
wwnz browse "Fruit & Veg"
```

No sign-in is needed for any of that — a guest token is minted and cached on
first use, and with no store bound the site prices against a default. The cart
and order history do need an account:

```bash
wwnz auth login --email you@example.com
wwnz auth status                        # is there a session?
wwnz cart add 282768 2
wwnz cart list
wwnz orders list
wwnz orders show 1
wwnz orders previous                    # the site's "buy it again"
```

Every command takes `--json`:

```bash
wwnz --json specials --limit 200 | jq -r '.products[] | select(.price_cents < 300) | .name'
wwnz --json auth status | jq .signed_in
```

## Commands

| Command | What it does |
| --- | --- |
| `search <query>` | Find products. `--limit`, `--size`, `--specials`, `--sort` |
| `specials` | Everything currently on promotion at your store |
| `browse <department>` | List a whole department, aisle or shelf |
| `departments [query]` | The department tree, or the subtree under one node |
| `stores [query]` | List stores by name, suburb or city |
| `store show\|set\|clear` | Show, choose or forget the store to price against |
| `config list\|get\|set\|unset\|path` | Read and change the settings file |
| `cart list` | Show the cart, its lines and the estimated total |
| `cart add <sku> [qty]` | Add to the cart; `--unit kg` for weight-priced items |
| `cart update <sku> <qty>` | Set a quantity outright; `0` removes the line |
| `cart remove <sku>` | Remove a product |
| `cart clear --force` | Empty the cart |
| `orders list` | Past orders, newest first. `--limit`, `--filter` |
| `orders show <#\|id>` | One order and what was in it |
| `orders previous` | What you have bought before, for buying it again |
| `auth login` / `auth logout` | Sign in through Auth0; forget the session |
| `auth status` | Whether there is a session, and how old it is |
| `auth refresh` | Sign in again from the stored password |
| `auth import <cookies.txt>` | Seed a session from a browser's Netscape cookies |
| `doctor` | Check config, credentials and connectivity; exits non-zero if unhealthy |
| `completions [shell]` | Print a completion script; the shell defaults to `$SHELL` |
| `update` | Install the newest release. `--check` reports without installing |

Global flag: `--json`. `wwnz -V` names the build; `wwnz --version` gives the
whole provenance and the version of every library it was compiled against.

## The store is server-side

Prices, specials and stock are per store, and on this site the store is a
property of the **cart**, not a local preference. `wwnz store set` is therefore
a mutation: it binds the account's cart to that store and records the id
locally so a listing can be headed with it.

The consequence is that there is no per-command store. `--store` is still
accepted by the commands that quote a price, and refused:

```
$ wwnz search milk --store 9048
wwnz: Woolworths does not support a per-command --store
      the Woolworths cart is bound to a store server-side; run `wwnz store set <store>` instead
```

Answering with one store's prices under another store's name would be a
wrong-price bug with nothing on screen to explain it, so it is refused rather
than ignored.

## Weighed lines

Loose produce and meat are sold by the kilogram — the `-KGM` variants — and
their quantities are **decimals in kilograms**, not counts:

```bash
wwnz cart add 144329 0.3 --unit kg      # 300g of brown onions
wwnz cart update 57133 1.5 --unit kg    # 1.5kg of nibbles
```

Everything else is a count. `cart add` tops up what is already there, in the
line's own unit: `cart add 144329 1` on a line sold by the kilogram means
another kilogram. `cart update` sets the quantity outright. Every mutation
prints the resulting cart.

Search prints a stock code and the API only accepts a variant key, so `cart add
282848` is completed to `282848-EA` before it is sent — the bare code is not
rejected as unknown, it is answered with "no longer available at this store",
which reads as the product being gone.

## Configuration

`~/.config/woolworths-nz-cli/config.toml` (written by `store set` / `config
set`, mode 0600, only the keys that were changed):

```toml
store_id = "9048"             # what `store set` bound the cart to

[auth]
password_command = "..."      # prints the account password; never written here
store_password = true         # keep the password in the credential store (default)

[output]
color = "auto"                # auto | always | never
```

`wwnz config list` prints every key, its value and what it does. Cached tokens
and the session live in `~/.local/state/woolworths-nz-cli/`.

Environment overrides, all optional and all beating the config file:

| Variable | Purpose |
| --- | --- |
| `WWNZ_SECRET_BACKEND` | `keyring` or `file`, overriding auto-detection |
| `WWNZ_ORIGIN` | Move the storefront, which also serves the GraphQL endpoint |
| `WWNZ_AUTH_ORIGIN` | Move the Auth0 host the login flow walks |
| `WWNZ_CONFIG_DIR`, `WWNZ_STATE_DIR` | Relocate config and state |
| `WWNZ_DEBUG_AUTH` | Narrate the login flow on stderr |
| `WWNZ_UPDATE_API` | Move the GitHub API base used by `wwnz update` |
| `GITHUB_TOKEN`, `GH_TOKEN` | Raise the rate limit on `wwnz update`; sent to github.com only |
| `NO_COLOR` | Disable colour whatever the config says |

`WWNZ_DEBUG_AUTH` prints no credential: query strings are dropped, because they
carry the flow's `state` token, and cookies appear by name only.

## Logging in

No browser is needed — `wwnz auth login` walks the Auth0 flow itself:

```bash
wwnz auth login --email you@example.com
```

Auth0 challenges nothing this flow can answer, so a challenged login fails
rather than prompting. `wwnz auth import cookies.txt` is the way in when that
happens: sign in with a browser, export its cookies for woolworths.co.nz, and
hand the file over. An imported session is nameless, and cannot be renewed for
the same reason — renewal needs an address to sign in with.

### Staying signed in

**A Woolworths session cannot be refreshed.** The session cookie is encrypted
and only the site can mint one, so the only renewal there is is walking the
whole login flow again — which takes a password. That is why the password is
**stored by default**, and why `--no-store-password` leaves a lapsed session
stopping every account command until someone signs in by hand.

A plaintext password in the credential store is a heavier thing to hold than a
session. `wwnz auth logout` removes it with everything else, and either of
these keeps it out of the store entirely:

```bash
wwnz auth login --email you@example.com --no-store-password
```

```toml
[auth]
store_password = false                                       # every login
password_command = "op read op://Personal/Woolworths/password"   # sign in from a manager instead
```

A command that finds its session lapsed signs itself back in and keeps the new
one, so a cron job costs one extra login rather than a prompt. `wwnz auth
refresh` does it now rather than on next use.

`wwnz auth status` answers without a request:

```
Woolworths  signed in you@example.com
  signed in 3h ago; renewable from the stored password
```

There is no expiry to print — the cookie is encrypted, so only how long ago it
was obtained is known. Sessions last about 24 hours.

## Past orders

Needs `wwnz auth login`: an order history belongs to an account.

```bash
wwnz orders list
wwnz orders list --limit 50 --filter active
wwnz orders show 1
wwnz orders previous
```

`orders show` takes the position from the listing or a real order number;
positions shift as new orders arrive, and `--json` carries the numbers.
`--filter in-store` is refused: Woolworths keeps no till-receipt history, only
its online orders.

`orders previous` is the site's "buy it again". It answers with products at
today's price rather than with historical lines, so the money there is what one
would cost now, not what was paid — that is the only thing the API offers.

## What is not implemented

**Checkout.** Timeslot reservation and order placement are deliberately absent:
they spend real money.

## Upgrading from 0.2

0.3 is the rebuild onto the shared libraries, and the surface moved with it:

- The config file is refused rather than silently ignored if it still uses the
  0.2 layout. `password_command` and `store_password` are now under `[auth]`,
  and `store_name` is gone — `store show` prints the id, and `doctor` names the
  store from the live list.
- `--store` and `WWNZ_STORE_ID` no longer pick a store per run. 0.2 quietly
  rebound the account's cart on every listing command; `store set` is now the
  only thing that does that. `--store` is refused with that advice, and
  `WWNZ_STORE_ID` is no longer read.
- `WWNZ_SESSION` and `WWNZ_EMAIL` are gone. `auth import` covers the first;
  `--email` covers the second.
- `orders show` exists now — the operation for one order's contents is
  implemented in `wwnz-api`.
- `--unit kg` replaces `--unit KGM`, and `auth status --json` is the status
  object rather than a wrapper.

## When Woolworths changes something

These endpoints are undocumented and unversioned, so expect breakage. Two
things make it survivable without a new release:

- **Every field is optional.** A renamed field becomes a missing column, not a
  failed command.
- **Every URL is overridable.** `WWNZ_ORIGIN` and `WWNZ_AUTH_ORIGIN` repoint
  the client at whatever the site is using now.

Start with `wwnz doctor`, which separates "session problem" from "API problem"
from "store not bound", and prints the version of `wwnz-api` that is compiled
in — the part that breaks when the API moves.

`wreq` is used rather than `reqwest`, and that is not incidental: Akamai sits
in front of woolworths.co.nz and scores the TLS handshake. With rustls the
storefront withholds its bot-manager cookies and the login is answered with a
bare 400; with a browser handshake the same requests are answered normally.
That client lives in [`net-kit`](../../packages/net-kit).

## Development

```bash
dispat run check --since all -p woolworths-nz-cli   # fmt, clippy, build, test
cargo test
cargo run --quiet -- search milk
```

The tests here are fast: flag precedence, the refusals, exit codes and the
shape of `--json`, none of them touching the network. Wire behaviour — the
login flow, request bodies, response parsing — is tested in
[`wwnz-api`](../../packages/wwnz-api) against its own mock server.
