# foodstuffs-nz-cli

Search New World and PAK'nSAVE from the terminal. Modelled on
[`woolies-nz-cli`](https://github.com/mcinteerj/woolies-nz-cli).

Both banners are Foodstuffs NZ and run the same platform, so one client drives
both. That is what makes `compare` possible: one query priced at both chains.

The other half of the duopoly is [`woolworths-nz-cli`](../woolworths-nz-cli),
built the same way against a different API. `compare` does not reach across to
it: the two companies share no SKUs, so there is nothing to join the rows on.

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
cargo build                        # from this directory
cargo install --path .             # or install the `fsnz` binary
```

`curl` has to be on `PATH` at runtime -- see
[Getting past Cloudflare](#getting-past-cloudflare). macOS, Windows 10 and
later, and most Linux distributions ship it.

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

## Getting past Cloudflare

Foodstuffs put Cloudflare bot management in front of the storefronts and the
Club Plus API, and it fingerprints the **connection**, not the headers. Two
things are rejected outright: HTTP/2 from anyone, and the TLS handshakes of
both rustls and macOS SecureTransport -- which is every backend `reqwest` can
be built with. OpenSSL and LibreSSL handshakes are accepted, which is to say
`curl` is accepted, so [`src/process/curl.rs`](src/process/curl.rs) shells out
to it for the requests that touch those hosts: the storefront token mint and
the Club Plus login. Bodies go in over stdin, never the command line, since the
login body holds a password and arguments are visible to `ps`.

Everything else uses `reqwest` normally.

The sibling [`woolworths-nz-cli`](../woolworths-nz-cli) hits the same class of
problem and solves it the opposite way, because Woolworths use Akamai and
Akamai scores `curl` *worse* than rustls. Do not copy this approach there, or
that one here.

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
| `auth refresh` | Mint fresh tokens, replacing the cached ones |
| `doctor` | Check config, token and connectivity; exits non-zero if unhealthy |
| `completions [shell]` | Print a completion script; the shell defaults to `$SHELL` |
| `update` | Install the newest release. `--check` reports without installing |

Global flags: `--banner`, `--store`, `--token`, `--json`.

`fsnz -V` names the build; `fsnz --version` gives the whole provenance.

## Shell completions

`fsnz completions` writes a completion script to stdout for `bash`, `zsh`,
`fish`, `powershell` or `elvish`, inferring the shell from `$SHELL` when you do
not name one. Try it for a session:

```bash
source <(fsnz completions bash)
source <(fsnz completions zsh)    # after compinit has run
```

In zsh the script calls `compdef`, which does not exist until `compinit` has
run -- sourcing before that fails with `command not found: compdef`. Any zshrc
that already sets up completion (oh-my-zsh included) has run it by then.

To keep it, put the script where the shell looks:

```bash
fsnz completions bash > ~/.local/share/bash-completion/completions/fsnz
fsnz completions zsh  > "${fpath[1]}/_fsnz"      # any directory on $fpath
fsnz completions fish > ~/.config/fish/completions/fsnz.fish
```

The script lists commands and flags only; it does not complete store names or
SKUs, which would mean a request per keystroke.

## Updating

`fsnz update` looks for the newest `foodstuffs-nz-cli/vX.Y.Z` tag in the
releases of the monorepo this lives in. It cannot use GitHub's own
`releases/latest`, which answers with the newest release of *any* project in
the repository.

```console
$ fsnz update --check
fsnz 0.1.1 -> 0.2.0 available
  https://github.com/jason-s13r/claude-playground/releases/tag/foodstuffs-nz-cli/v0.2.0
  run `fsnz update` to install foodstuffs-nz-cli-0.2.0-linux-x86_64.tar.gz
```

`--check` exits non-zero when there is something newer, so it can gate a script
the way `doctor` does. A preview is only ever mentioned, never counted, so it
cannot flip that exit code.

Which releases are on offer follows from the running version, and nothing is
remembered between runs:

| Running | `fsnz update` moves to |
| ------- | ---------------------- |
| a stable release | the newest stable release |
| a prerelease | the newest stable, if one is ahead; otherwise the next preview |

So a preview build walks forward through the previews and rejoins the stable
channel as soon as a stable release passes it.

```console
$ fsnz update --pre-release      # newest release of either channel
$ fsnz update 0.1.4-rc.2         # exactly that one; `v` optional
$ fsnz update 0.1.3              # explicit downgrade
```

Naming a version does not pin anything: the next plain `fsnz update` follows
the table above from wherever that left the binary.

Installing downloads the tarball built for this machine, checks it against the
release's `SHA256SUMS` and refuses to go on if it does not match, then replaces
the running binary in place. That needs write access to the directory the
binary lives in -- a `/usr/local/bin` install wants `sudo`; `~/.local/bin` and
`~/.cargo/bin` do not, which is also what to use on an immutable distro like
Bazzite or Silverblue. Nothing else on the machine is touched.

The Linux binary is built on Ubuntu 24.04 and links glibc dynamically, so it
runs on any distro with glibc 2.39 or newer -- current Fedora, Arch, Debian 13,
and the Fedora-derived immutable desktops. Older distros should build from
source. The macOS binary is unsigned; `fsnz update` is unaffected, but a copy
downloaded through a browser needs `xattr -d com.apple.quarantine fsnz` before
it will run.

Afterwards `fsnz --version` says where the binary came from:

```console
$ fsnz --version
fsnz 0.2.0
commit     9f2c1ab34 (2026-08-30)
source     jason-s13r/claude-playground, release tag foodstuffs-nz-cli/v0.2.0
built by   GitHub Actions, from the release workflow
build      release, x86_64-unknown-linux-gnu, rustc 1.94.0
binary     /home/you/.local/bin/fsnz
installed  by `fsnz update` from foodstuffs-nz-cli/v0.2.0 on 2026-08-30
```

A binary built from a working tree says so instead, down to whether the tree
had uncommitted changes in it. The install record is a small file at
`~/.local/state/foodstuffs-nz-cli/install.json`; deleting it only removes the
last line.

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
| `FSNZ_UPDATE_API` | Move the GitHub API base used by `fsnz update` |
| `GITHUB_TOKEN`, `GH_TOKEN` | Raise the rate limit on `fsnz update`; sent to github.com only |

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

PAK'nSAVE
  token        none cached; minted on next use
  linked       no
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

`fsnz auth refresh` throws the cached tokens away and mints replacements. It
reports what it minted rather than printing them: no command prints a token in
human output, so a JWT never lands in scrollback or shell history by accident.
Scripts that genuinely need the value read it from the JSON:

```bash
fsnz --json auth status | jq -r '.banners.newworld.token'
```

That reads the cache without minting, so it is `null` until something has
warmed it -- `fsnz auth refresh` first if you need a value there and then.

One account covers both banners, so `auth` works across both by default:
`login` proves the session at each, `refresh` mints for each, and `status`
reports each. `-b` narrows any of them to the banner it names.

```bash
fsnz auth refresh              # both banners
fsnz -b pns auth refresh       # PAK'nSAVE only
```

`refresh` treats the banners independently: one failing is reported against
that banner and the other still mints, so it only exits non-zero when every
banner failed. `auth logout` is the exception to all of this and ignores `-b`
-- there is a single Club Plus session behind both banners, so there is no
half of it to drop.

`fsnz doctor` shows who is logged in and where the session is kept.

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
dispat run check --since all -p foodstuffs-nz-cli   # fmt, clippy, build, test
cargo test
cargo run --quiet -- search milk
```

The tests run the real binary against a mock Foodstuffs (`wiremock`) with
`FSNZ_*_API`/`FSNZ_*_ORIGIN` pointed at it, so the whole path — token minting
and caching, request bodies, response parsing, rendering, exit codes — is
covered without touching the network.
