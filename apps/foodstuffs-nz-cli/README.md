# foodstuffs-nz-cli

Search, compare and shop New World and PAK'nSAVE from the terminal.

Both banners are Foodstuffs NZ and run the same platform, so one client drives
both. That is what makes `compare` possible: one query priced at both chains.

> **Not affiliated with Foodstuffs New Zealand, New World or PAK'nSAVE.** There
> is no public API. This calls the same undocumented endpoints their websites
> call from the browser, and can break whenever they change something. Use at
> your own risk.

## How it is built

`fsnz` is a thin front end. The parts worth reading are in `packages/`:

| Crate | What it holds |
| --- | --- |
| [`gsnz-core`](../../packages/gsnz-core) | the grocery domain: one `Product`, `Cart`, `Order`, `Store`, and the `Retailer` trait |
| [`fsnz-api`](../../packages/fsnz-api) | the Foodstuffs edge API and the Club Plus login, in its own vendor-shaped types |
| [`cli-kit`](../../packages/cli-kit) + [`gsnz-ui`](../../packages/gsnz-ui) | tables, `--json`, and the grocery renderers |
| [`net-kit`](../../packages/net-kit) | the browser-fingerprinted HTTP client, the cookie jar and the credential store |
| [`build-kit`](../../packages/build-kit) | the build stamp and `fsnz update` |

What is left in `src/` is the part that is genuinely this program: reading the
environment once, resolving flags against config, adapting `fsnz-api` types to
`gsnz-core`, and turning a failure into an exit code.

[`grocery-nz-cli`](../grocery-nz-cli) (`gsnz`) is the same architecture with
Woolworths added as a second `Retailer`; `fsnz` is the Foodstuffs-only slice.

## Install

```bash
cargo build                        # from this directory
cargo install --path .             # or install the `fsnz` binary
```

Or take a published build from
[releases](https://github.com/jason-s13r/claude-playground/releases), which are
tagged `foodstuffs-nz-cli/vX.Y.Z`. Once you have a binary it can replace itself:

```bash
fsnz update --check     # is there a newer one?
fsnz update             # download it and swap it in
```

Releases publish `linux-x86_64` and `darwin-arm64` binaries. On anything else
`fsnz update` says what the release does have and leaves the binary alone;
build from source instead.

## Quick start

Prices, specials and stock are per store. Pick one first.

```bash
fsnz auth login --email you@example.com      # once; no browser needed
fsnz -b nw stores wellington                 # find a store
fsnz -b nw store set "New World Thorndon"    # remember it
fsnz -b nw search milk
fsnz -b nw search milk --size 2L --limit 5
fsnz -b nw specials --limit 40
fsnz -b nw browse "Fruit & Vegetables"
```

`-b`/`--banner` takes `nw` or `pns`. Set a default so bare commands have a
banner:

```bash
fsnz use nw                     # or: fsnz config set banner nw
fsnz search milk                # now talks to New World
```

The other banner:

```bash
fsnz -b pns store set "PAK'nSAVE Kilbirnie"
fsnz -b pns search milk
```

With a store set at both:

```bash
fsnz compare milk
```

```
┌───────────────────────────┬───────┬───────────┬───────────┬────────────┐
│ Product                   ┆ Size  ┆ New World ┆ PAK'nSAVE ┆ Difference │
╞═══════════════════════════╪═══════╪═══════════╪═══════════╪════════════╡
│ Anchor Blue Milk          ┆ 1l    ┆ $3.73     ┆ $3.57  ←  ┆ $0.16      │
│ Anchor Blue Milk          ┆ 2l    ┆ $5.79     ┆ $5.69  ←  ┆ $0.10      │
│ Pams Value Standard Milk  ┆ 3l    ┆ $7.19     ┆ $7.11  ←  ┆ $0.08      │
│ Anchor Calci + Trim Milk  ┆ 1l    ┆ $3.79     ┆ —         ┆            │
└───────────────────────────┴───────┴───────────┴───────────┴────────────┘
```

Products found at both come first, biggest price gap at the top. `—` means the
product was not in that banner's results, which is not the same as unavailable.
Rows are joined on SKU, which the two banners share; `--strict` drops rows that
were paired by description instead.

Every command takes `--json`:

```bash
fsnz --json -b nw specials --limit 200 | jq -r '.products[] | select(.price_cents < 300) | .name'
fsnz --json compare bread | jq -r '.rows[]'
```

## Commands

| Command | What it does |
| --- | --- |
| `search <query>` | Find products. `--limit`, `--size`, `--specials`, `--sort` |
| `specials` | Everything currently on promotion at your store |
| `browse <department>` | List a whole department, e.g. `"Fruit & Vegetables"` |
| `departments [query]` | The department tree, or the subtree under one node |
| `compare <query>` | The same search at both banners, side by side. `--strict` |
| `stores [query]` | List stores, optionally filtered by name |
| `store show\|set\|clear` | Show, choose or forget the store to price against |
| `use [banner]` | Show or set the default banner |
| `config list\|get\|set\|unset\|path` | Read and change the settings file |
| `cart list` | Show the cart, its lines and the estimated total |
| `cart add <sku> [qty]` | Add to the cart; `--unit kg` for weight-priced items |
| `cart update <sku> <qty>` | Set a quantity outright; `0` removes the line |
| `cart remove <sku>` | Remove a product |
| `cart clear --force` | Empty the cart |
| `orders list` | Past orders, newest first. `--limit`, `--filter` |
| `orders show <#\|id>` | One order and what was in it |
| `orders previous` | What you have bought before, for buying it again |
| `auth login` / `auth logout` | Sign in through Club Plus; forget the session |
| `auth status` | Who is signed in, and for how much longer |
| `auth refresh` | Renew the session without a full sign-in |
| `auth import <cookies.txt>` | Seed a session from a browser's Netscape cookies |
| `doctor` | Check config, token and connectivity; exits non-zero if unhealthy |
| `completions [shell]` | Print a completion script; the shell defaults to `$SHELL` |
| `update` | Install the newest release. `--check` reports without installing |

Global flags: `-b`/`--banner`, `--json`. `--store` is taken only by the
commands that quote a price (`search`, `specials`, `browse`, `compare`,
`departments`).

`fsnz -V` names the build; `fsnz --version` gives the whole provenance and the
version of every library it was compiled against.

## Configuration

`~/.config/foodstuffs-nz-cli/config.toml` (written by `store set` / `config
set`, mode 0600, only the keys that were changed):

```toml
banner = "paknsave"          # default banner when -b is not given

[auth]
password_command = "..."     # prints the Club Plus password; never written here
store_password = true         # keep the password in the credential store (default)

[compare]
match = "normalised"          # or "exact" -- pair only on shared product code

[output]
color = "auto"                # auto | always | never

[newworld]
store_id = "..."

[paknsave]
store_id = "..."
token_command = "..."        # shell command printing a bearer token on stdout
```

`fsnz config list` prints every key, its value and what it does.

Cached tokens live in `~/.local/state/foodstuffs-nz-cli/`.

Environment overrides, all optional and all beating the config file:

| Variable | Purpose |
| --- | --- |
| `FSNZ_BANNER` | Default banner |
| `FSNZ_NEWWORLD_STORE_ID`, `FSNZ_PAKNSAVE_STORE_ID` | Store, without touching the config file |
| `FSNZ_NEWWORLD_TOKEN`, `FSNZ_PAKNSAVE_TOKEN` | A bearer token supplied outright, per banner |
| `FSNZ_SECRET_BACKEND` | `keyring` or `file`, overriding auto-detection |
| `FSNZ_NEWWORLD_API`, `FSNZ_PAKNSAVE_API` | Move the API base URL |
| `FSNZ_NEWWORLD_ORIGIN`, `FSNZ_PAKNSAVE_ORIGIN` | Move the storefront URL |
| `FSNZ_CLUBPLUS_API`, `FSNZ_CLUBPLUS_ORIGIN` | Move the Club Plus endpoints |
| `FSNZ_CONFIG_DIR`, `FSNZ_STATE_DIR` | Relocate config and state |
| `FSNZ_UPDATE_API` | Move the GitHub API base used by `fsnz update` |
| `GITHUB_TOKEN`, `GH_TOKEN` | Raise the rate limit on `fsnz update`; sent to github.com only |
| `NO_COLOR` | Disable colour whatever the config says |

## Logging in

Foodstuffs accounts sit behind Club Plus. No browser is needed:

```bash
fsnz auth login --email you@example.com
```

Club Plus may hold a login from a device it does not recognise and email a
one-time code; `fsnz auth login` asks for it. Nothing is stored until the code
is accepted, so an interrupted verification leaves no half-finished login
behind. The code cannot be a flag -- it does not exist until the login that
demands it has been made -- so where there is no terminal it is read from
stdin:

```bash
printf '%s\n' "$CODE" | fsnz auth login --email you@example.com --password-command 'pass clubplus'
```

One Club Plus account covers both banners, so `fsnz auth login` is a single
prompt and signs in at both. `fsnz auth status` shows where things stand
without making a request:

```
New World   signed in you@example.com
  expires in 24m
PAK'nSAVE   signed in you@example.com
  expires in 24m
```

### Staying signed in

The Club Plus session lasts about 30 minutes. Any command needing an account
token renews it first if it has aged out. That refresh **rotates** the refresh
token, so `fsnz` writes the replacement to the credential store before using
the session -- losing it is what ends a session, and a refresh token used
elsewhere invalidates the stored one.

Once the refresh token is spent, `fsnz` signs in again from a password instead
-- `[auth] password_command` if one is configured, otherwise the copy `auth
login` kept in the credential store. That is what lets a cron job keep working:
a spent refresh token costs one extra login rather than a prompt. It cannot
answer a device-verification code, so a challenged login still needs `fsnz auth
login` at a keyboard.

**The password is stored by default**, alongside the session. It is a
plaintext password in the credential store, which is a heavier thing to hold
than a half-hour session -- `fsnz auth logout` removes it with everything else,
and either of these keeps it out of the store entirely:

```bash
fsnz auth login --email you@example.com --no-store-password
```

```toml
[auth]
store_password = false                                          # every login
password_command = "op read op://Personal/Club Plus/password"   # renew from a manager instead
```

`fsnz auth refresh` renews the session now rather than on next use. `fsnz auth
logout` drops the session, the cookies and the stored password -- it ignores
`-b`, because there is one Club Plus session behind both banners.

### Without signing in

The read APIs only need a bearer token, which can be supplied directly:

```bash
export FSNZ_NEWWORLD_TOKEN='<value>'      # New World
export FSNZ_PAKNSAVE_TOKEN='<value>'      # PAK'nSAVE
```

or as `[newworld] token_command` / `[paknsave] token_command` in the config.
Get one from DevTools → Application → Cookies → `fs-user-token`; it lasts about
30 minutes. Tokens are scoped to one banner: the API rejects a New World token
presented with a PAK'nSAVE store, and a mis-scoped token answers the cart
endpoints with an empty cart belonging to nobody.

`fsnz auth import cookies.txt` seeds a session from a Netscape `cookies.txt`
exported from a browser signed in to the banner.

## The cart

Needs `fsnz auth login`: a cart belongs to an account, not a store. `fsnz -b nw
store set` also binds that store to the account's cart, which is what makes
`cart add` work.

```bash
fsnz -b nw cart add 5039956-EA-000          # one broccoli
fsnz -b nw cart add 5101189-KGM-000 0.3 --unit kg    # 300g of beef mince
fsnz -b nw cart update 5034758-EA-000 2
fsnz -b nw cart remove 5107154-EA-000
fsnz -b nw cart list
```

```
┌──────┬───────────────────────┬─────────────────┬────────────┐
│ Qty  ┆ Product               ┆ SKU             ┆ Line total │
╞══════╪═══════════════════════╪═════════════════╪════════════╡
│ 1    ┆ Broccoli              ┆ 5039956-EA-000  ┆ $1.79      │
│ 0.3kg┆ NZ Premium Beef Mince ┆ 5101189-KGM-000 ┆ $7.20      │
└──────┴───────────────────────┴─────────────────┴────────────┘
  Subtotal                   $8.99
  Bag fee                    $1.50
  Estimated total           $10.49
```

Weight-priced produce (`-KGM-` in the SKU) is counted in kilograms with `--unit
kg`; counted items (`-EA-`) default to one. `cart add` tops up what is already
there; `cart update` sets the quantity outright. Every mutation prints the
resulting cart.

## Past orders

Needs `fsnz auth login`, for the same reason the cart does. Two kinds show up
together: orders placed online, and till receipts from shopping in a store,
which Foodstuffs links to the account through Club Plus.

```bash
fsnz -b nw orders list
fsnz -b nw orders list --limit 50 --filter in-store
fsnz -b nw orders show 1
fsnz -b nw orders previous
```

Order ids are 150 characters of path, so `orders show` takes the number from
the listing instead. Positions shift as new orders arrive; `--json` carries the
real ids, and `orders show` accepts one of those too.

An online order carries more than an in-store one: its status, timeslot,
delivery address and the fees, which are why its lines do not add up to the
total on their own.

`orders previous` is the site's "buy it again": what this account has bought
before, with what it cost at the time. The SKUs are the ones `fsnz cart add`
takes, so a past order is a shopping list.

## What is not implemented

**Checkout.** Timeslot reservation and order placement are deliberately absent:
they spend real money. Shopping lists are exposed by the API but not
implemented.

## When Foodstuffs changes something

These endpoints are undocumented and unversioned, so expect breakage. Two
things make it survivable without a new release:

- **Every field is optional.** A renamed field becomes a missing column, not a
  failed command.
- **Every URL is overridable.** `FSNZ_*_API` and `FSNZ_*_ORIGIN` repoint the
  client at whatever the site is using now.

Start with `fsnz doctor`, which separates "token problem" from "API problem"
from "store not selected", and prints the version of `fsnz-api` that is
compiled in -- the part that breaks when the API moves.

## Development

```bash
dispat run check --since all -p foodstuffs-nz-cli   # fmt, clippy, build, test
cargo test
cargo run --quiet -- -b nw search milk
```

The tests here are fast: flag precedence, the refusals, exit codes and the
shape of `--json`, none of them touching the network. Wire behaviour -- token
minting and caching, request bodies, response parsing -- is tested in
[`fsnz-api`](../../packages/fsnz-api) against its own mock server.
