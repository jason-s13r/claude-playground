# foodstuffs-nz-cli

Search New World and PAK'nSAVE from the terminal. Modelled on
[`woolies-nz-cli`](https://github.com/mcinteerj/woolies-nz-cli).

Both banners are Foodstuffs NZ and run the same platform, so one client drives
both. That is what makes `compare` possible: one query priced at both chains.

> **Not affiliated with Foodstuffs New Zealand, New World or PAK'nSAVE.** There
> is no public API. This calls the same undocumented endpoints their websites
> call from the browser, and can break whenever they change something. Use at
> your own risk.

## Status

Verified against a real account: `auth login`, `auth status`, `stores`,
`search`, `compare`, and `cart list` at both banners. Cart writes (`add`,
`update`, `remove`, `clear`) are implemented but untested against the live API.
So is `orders`, whose in-store half is built from a recorded session; the online
half shares the endpoint's shape but no online order was there to try it on.

## Install

```bash
make build P=foodstuffs-nz-cli     # from the repo root
cargo install --path .             # or install the `fsnz` binary
```

## Quick start

Prices, specials and stock are per store. Pick one first.

```bash
fsnz auth login --email you@example.com      # once; no browser needed
fsnz stores wellington                  # find a store
fsnz store set "New World Thorndon"     # remember it
fsnz search milk
fsnz search milk --size 2L --limit 5
fsnz specials --limit 40
fsnz browse "Fruit & Vegetables"
```

The other banner:

```bash
fsnz --banner pns stores wellington
fsnz --banner pns store set "PAK'nSAVE Kilbirnie"
fsnz --banner pns search milk
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
│ Pams Value Standard Milk  ┆ 1l    ┆ $3.16     ┆ $3.14  ←  ┆ $0.02      │
│ Anchor Calci + Trim Milk  ┆ 1l    ┆ $3.79     ┆ —         ┆            │
└───────────────────────────┴───────┴───────────┴───────────┴────────────┘

7 products compared, 4 found at both. ← marks the cheaper banner.
```

Products found at both come first, biggest price gap at the top. `—` means the
product was not in that banner's results, which is not the same as unavailable.
Rows are joined on SKU, which the two banners share.

Every command takes `--json`:

```bash
fsnz --json specials --limit 200 | jq -r '.products[] | select(.price < 3) | .name'
fsnz --json compare bread | jq -r '.rows[] | select(.cheapest == "paknsave") | .title'
```

## Commands

| Command | What it does |
| --- | --- |
| `search <query>` | Find products. `--limit`, `--size`, `--specials`, `--sort` |
| `specials` | Everything currently on promotion at your store |
| `browse <department>` | List a whole department, e.g. `"Fruit & Vegetables"` |
| `compare <query>` | The same search at both banners, side by side |
| `stores [query]` | List stores, optionally filtered by name |
| `store show\|set\|clear` | Show, choose or forget the store to price against |
| `cart list` | Show the cart, its lines and the estimated total |
| `cart add <sku> [qty]` | Add to the cart; grams for weight-priced items |
| `cart update <sku> <qty>` | Set a quantity outright; `0` removes the line |
| `cart remove <sku>` | Remove a product |
| `cart clear --force` | Empty the cart |
| `orders list` | Past orders, newest first. `--limit`, `--source` |
| `orders show <#\|id>` | One order and what was in it |
| `orders previous` | What you have bought before, for buying it again |
| `auth login` / `auth logout` | Sign in through Club Plus; forget the session |
| `auth status` | Session, renewal and each banner's token; exits non-zero without one |
| `auth token [--refresh] [--raw]` | Show the token this tool would use |
| `doctor` | Check config, token and connectivity; exits non-zero if unhealthy |

Global flags: `--banner`, `--store`, `--token`, `--json`.

## Configuration

`~/.config/foodstuffs-nz-cli/config.toml` (written by `store set`, mode 0600):

```toml
banner = "paknsave"          # default banner when --banner is not given
password_command = "..."     # prints the Club Plus password; never stored here

[newworld]
store_id = "..."

[paknsave]
store_id = "..."
token_command = "..."        # shell command printing a token on stdout
```

Cached tokens live in `~/.local/state/foodstuffs-nz-cli/`.

Environment overrides, all optional:

| Variable | Purpose |
| --- | --- |
| `FSNZ_BANNER` | Default banner |
| `FSNZ_NEWWORLD_STORE_ID`, `FSNZ_PAKNSAVE_STORE_ID` | Store, without touching the config file |
| `FSNZ_TOKEN` | Use this token instead of minting one (single-banner commands) |
| `FSNZ_NEWWORLD_TOKEN`, `FSNZ_PAKNSAVE_TOKEN` | Per-banner tokens, required by `compare` |
| `FSNZ_EMAIL` | Default Club Plus email for `fsnz auth login` |
| `FSNZ_SECRET_BACKEND` | `keyring` or `file`, overriding auto-detection |
| `FSNZ_NEWWORLD_API`, `FSNZ_PAKNSAVE_API` | Move the API base URL |
| `FSNZ_NEWWORLD_ORIGIN`, `FSNZ_PAKNSAVE_ORIGIN` | Move the storefront URL |
| `FSNZ_CLUBPLUS_API`, `FSNZ_CLUBPLUS_LOGIN` | Move the Club Plus endpoints |
| `FSNZ_CONFIG_DIR`, `FSNZ_STATE_DIR` | Relocate config and state |

## Logging in

Foodstuffs accounts sit behind Club Plus. No browser is needed:

```bash
fsnz auth login --email you@example.com
```

Four calls: fetch the login API's public bearer token; exchange email and
password for a Club Plus session; mint a single-use code scoped to one banner;
swap that code for the banner's token. The result is checked at both banners,
since one account covers both, and both tokens are cached.

### Staying logged in

The Club Plus session lasts about 30 minutes -- the same clock as the banner
tokens minted from it -- so it is renewed automatically rather than asked for
again. Any command needing an account token renews the session first if it has
aged out, via `POST {clubplus api}/user/login/refresh`.

That endpoint **rotates** the refresh token: the reply carries a replacement and
the one just sent stops working. `fsnz` writes the replacement to the credential
store before using the session, because losing it means a password prompt. It
also means a refresh token used elsewhere invalidates the stored one -- the
symptom is `Club Plus would not renew the session (401)`, and the fix is
`fsnz auth login`.

`fsnz auth status` shows where things stand without making a request:

```
Club Plus
  account      you@example.com
  stored in    the system credential store
  session      valid for 24m
  renewal      automatic, from the stored refresh token
  linked to    MNW

New World
  token        cached, expires in 24m
  scope        MNW; cart available
  linked       yes
```

`scope` is the token's own `banner` claim, and it is the one worth checking: a
`NAT` token is accepted by the cart endpoints and answers with an empty cart
belonging to nobody. `linked` reports the session's `linkedAccounts` claim as-is
-- it does **not** predict whether a banner works, since an account listing
`MNW` alone still reads its PAK'nSAVE cart back fine.

The session is kept in the operating system's credential store (Keychain,
Credential Manager, Secret Service). Where there is none it falls back to a
0600 file and says so.

**The password is never stored.** Point `password_command` at a password
manager to avoid retyping it:

```toml
password_command = "op read op://Personal/Club Plus/password"
```

`fsnz auth logout` forgets the session and every cached token. `fsnz doctor` shows
who is logged in and where the session is kept.

### Without logging in

The read APIs only need a token, which can be supplied directly:

```bash
export FSNZ_TOKEN='<value>'                    # one banner
export FSNZ_NEWWORLD_TOKEN='...'               # both, for `compare`
export FSNZ_PAKNSAVE_TOKEN='...'
```

Get one from DevTools → Application → Cookies → `fs-user-token`. It lasts about
30 minutes.

Tokens are scoped to one banner: the API rejects a New World token presented
with a PAK'nSAVE store. `--token`/`FSNZ_TOKEN` therefore applies only to
commands talking to a single banner; `compare` and `doctor` need the per-banner
variables.

## The cart

Needs `fsnz auth login`: a cart belongs to an account, not a store.

```bash
fsnz cart add 5039956-EA-000          # one broccoli
fsnz cart add 5101189-KGM-000 300     # 300g of beef mince
fsnz cart update 5034758-EA-000 2
fsnz cart remove 5107154-EA-000
fsnz cart list
```

```
┌──────┬───────────────────────┬─────────────────┬────────────┐
│ Qty  ┆ Product               ┆ SKU             ┆ Line total │
╞══════╪═══════════════════════╪═════════════════╪════════════╡
│ 1    ┆ Broccoli              ┆ 5039956-EA-000  ┆ $1.79      │
│ 300g ┆ NZ Premium Beef Mince ┆ 5101189-KGM-000 ┆ $7.20      │
└──────┴───────────────────────┴─────────────────┴────────────┘
  Subtotal                   $8.99
  Bag fee                    $1.50
  Estimated total           $10.49
```

Weight-priced produce takes its quantity in **grams**, inferred from the SKU:
`-KGM-` is sold by the kilogram, `-EA-` by the each. So `cart add <kgm sku>`
refuses to guess a quantity, while `cart add <ea sku>` defaults to one.
`--unit units|weight` overrides the inference.

`cart add` tops up what is already in the cart; `cart update` sets the quantity
outright. Every mutation prints the resulting cart.

The cart carries its own store, separate from the one `fsnz store set` prices
against. `fsnz` reports a mismatch rather than reconciling it, and does not bind
the cart's store.

## Past orders

Needs `fsnz auth login`, for the same reason the cart does. Two kinds show up
together: orders placed online, and till receipts from shopping in a store,
which Foodstuffs links to the account through Club Plus.

```bash
fsnz orders list
fsnz orders list --limit 50 --source in-store
fsnz orders show 1
fsnz orders previous
```

```
New World — 4 orders

┌───┬──────────────────┬────────────────────┬──────────┬────────┐
│ # ┆ Placed           ┆ Store              ┆ Where    ┆ Total  │
╞═══╪══════════════════╪════════════════════╪══════════╪════════╡
│ 1 ┆ 2026-08-01 16:00 ┆ New World Thorndon ┆ in store ┆ $16.20 │
│ 2 ┆ 2026-07-01 16:00 ┆ New World Thorndon ┆ in store ┆ $58.30 │
│ 3 ┆ 2026-06-01 16:00 ┆ New World Thorndon ┆ in store ┆ $24.95 │
│ 4 ┆ 2026-05-01 16:00 ┆ New World Thorndon ┆ in store ┆ $71.05 │
└───┴──────────────────┴────────────────────┴──────────┴────────┘
Show one: fsnz orders show <#>
```

Order ids are 150 characters of path, so `orders show` takes the number from
that listing instead. Positions are relative to the listing, so they shift as
new orders arrive; `--json` carries the real ids, and `orders show` accepts one
of those too.

```
$ fsnz orders show 1
New World Thorndon

Placed 2026-08-01 16:00 · in store
Id: region/fsni/banner/NW/customer/1234567890/salesstaginglink/_S_000001234_...

┌─────┬─────────────────────────────────────────┬────────────────┬────────────┐
│ Qty ┆ Product                                 ┆ SKU            ┆ Line total │
╞═════╪═════════════════════════════════════════╪════════════════╪════════════╡
│ 2   ┆ Whittaker's Creamy Milk Chocolate Block ┆ 5011234-EA-000 ┆ $13.00     │
│ 1   ┆ Pams Wholegrain Toast Bread             ┆ 5019876-EA-000 ┆ $3.20      │
└─────┴─────────────────────────────────────────┴────────────────┴────────────┘
  Total                     $16.20

2 lines, $16.20
```

An online order carries more: its status, timeslot, delivery address and the
fees, which are why its lines do not add up to the total on their own.

`orders previous` is the site's "buy it again": what this account has bought
before, with what it cost at the time, not today. Products already in the cart
are left out unless `--include-cart` says otherwise.

```
┌─────┬─────────────────────────────────────────┬─────────────────┬───────────┐
│ Qty ┆ Product                                 ┆ SKU             ┆ Last paid │
╞═════╪═════════════════════════════════════════╪═════════════════╪═══════════╡
│ 1kg ┆ Pams Whole Almonds                      ┆ 5101234-KGM-000 ┆ $32.00    │
│ 1   ┆ Whittaker's Creamy Milk Chocolate Block ┆ 5011234-EA-000  ┆ $6.50     │
└─────┴─────────────────────────────────────────┴─────────────────┴───────────┘
What it cost last time, not today. Buy one again: fsnz cart add <sku>
```

The SKUs are the ones `fsnz cart add` takes, so a past order is a shopping list.

## What is not implemented

**Checkout.** Timeslot reservation and order placement are deliberately absent:
they spend real money. The endpoints are known if that changes.

Shopping lists are exposed by the API but not implemented.

## When Foodstuffs changes something

These endpoints are undocumented and unversioned, so expect breakage. Two things
make it survivable without a new release:

- **Every field is optional.** A renamed field becomes a missing column, not a
  failed command.
- **Every URL is overridable.** `FSNZ_*_API` and `FSNZ_*_ORIGIN` repoint the
  client at whatever the site is using now.

Start with `fsnz doctor`, which separates "token problem" from "API problem"
from "store not selected".

## Development

```bash
make check P=foodstuffs-nz-cli    # fmt, clippy, build, test
make test  P=foodstuffs-nz-cli
make run   P=foodstuffs-nz-cli ARGS="search milk"
```

The tests run the real binary against a mock Foodstuffs (`wiremock`) with
`FSNZ_*_API`/`FSNZ_*_ORIGIN` pointed at it, so the whole path — token minting
and caching, request bodies, response parsing, rendering, exit codes — is
covered without touching the network.
