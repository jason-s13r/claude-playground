# briscoe-group-nz-cli

`bgnz` — **Briscoes and Rebel Sport New Zealand** from the command line. Search
either catalogue, read a product, ask whether a shop has your size on the
shelf, and — signed in — work the cart, the wishlist, your orders and your
loyalty balance.

> Unofficial. Reverse-engineered from the sites' own traffic; there is no public
> API and these endpoints can change without notice.

## One binary, two fascias

Briscoes and Rebel Sport are one Magento deployment behind a `store` request
header, so `-b` switches between them:

```bash
bgnz search "filter jug"              # Briscoes, the default
bgnz -b rebel search "asics gel"      # Rebel Sport
bgnz config set banner rebel          # make that the default
```

It is a switch, not a fan-out. The two share a backend but not a catalogue —
the SKU namespaces are disjoint, `1xxxxxx` and `8xxxxxx` — so there is no
`compare` here. They do not share a sign-in either: separate Gigya sites,
separate credentials, and `auth status` always shows both.

## What it does

```bash
bgnz search "filter jug" --limit 5 --facets
bgnz categories kitchen --depth 3
bgnz browse 141 --sort PRICE_ASC
bgnz product 1103832                  # or paste the product URL
bgnz stores auckland --collect
bgnz store set 291
bgnz stock 1103832                    # can I collect it, and when
```

Signed in, per fascia:

```bash
bgnz -b rebel auth login
bgnz cart list / add / set / remove / coupon
bgnz cart collect                     # can the whole basket be picked up
bgnz wishlist list / add / remove
bgnz orders list                      # bought online
bgnz orders receipts                  # bought in a shop — a different history
bgnz orders show 30314022             # shipments, with carrier tracking
bgnz loyalty
```

Everything takes `--json`.

### Stock is per size

A configurable product has no barcode of its own — only its variants do — and
the collection service identifies a product by barcode. So `bgnz stock` on a
Rebel Sport shoe answers per size:

```
Rebel Sport Albany
┌────────────┬──────────────────────┬────────┬─────────────────────┐
│ SKU        ┆ Variant              ┆ Status ┆ Collection          │
╞════════════╪══════════════════════╪════════╪═════════════════════╡
│ 8237741004 ┆ White/Red / US10     ┆ OOS    ┆ Pick up in 2-4 days │
│ 8237741006 ┆ White/Red / US11     ┆ OOS    ┆ Pick up in 2-4 days │
└────────────┴──────────────────────┴────────┴─────────────────────┘
8 variants.
```

`bgnz cart collect` answers one verdict for the whole basket, because that is
what the service answers.

### Two order histories

`orders list` is what was bought online, out of the storefront, with carrier
tracking. `orders receipts` is what was bought in a shop, out of SAP, matched
to the loyalty membership and carrying the store and receipt reference. They
share no identifiers.

## Signing in

Gigya refuses a sign-in without a reCAPTCHA token, and refuses it *before* it
checks the password, so there is no HTTP-only path. `auth login` drives a
browser to mint that token:

```bash
bgnz auth login                       # headless
bgnz auth login --headful             # if a headless run is refused
```

The browser's whole job is the token. **The email and password never reach
it** — the sign-in request is made by this program once the token is in hand,
which also lets it ask for a session that outlives a browser tab. It needs
[camoufox](https://camoufox.com):

```bash
uv tool install "camoufox[geoip]" && camoufox fetch
```

**Once per fascia.** What gets stored is Gigya's session token; everything
after that — including a fresh storefront token every two hours — renews with
no browser and no password.

### Or skip the browser

The credential is a plain cookie on the site, so it can be copied out of
devtools:

```bash
bgnz auth token --cookie       # prints glt_4_R5DCbpg6mrfoot48AR2Wcg
bgnz auth token                # paste the value, hidden
```

## Setup

```bash
bgnz doctor                    # what is set up, for both fascias
bgnz config list
source <(bgnz completions zsh)
```

| Setting | What it is |
| --- | --- |
| `banner` | the fascia a bare command talks to |
| `store.briscoes` | the shop `stock` asks about, by store id |
| `store.rebelsport` | the same, for the other fascia |
| `output.color` | `auto`, `always`, `never` |

Credentials go to the system keychain where there is one, and to a `0600` file
otherwise — filed per fascia, so `auth logout` on one leaves the other alone.

### Environment

| Variable | What it overrides |
| --- | --- |
| `BGNZ_CONFIG_DIR`, `BGNZ_STATE_DIR` | where settings and credentials live |
| `BGNZ_SECRET_BACKEND` | `keyring` or `file` |
| `BGNZ_BRISCOES_ORIGIN`, `BGNZ_REBEL_ORIGIN`, `BGNZ_GIGYA_ORIGIN` | the hosts, for tests |
| `BGNZ_EMULATION` | the browser profile the client presents as |
| `BGNZ_BROWSER_PYTHON` | the interpreter that can `import camoufox` |
| `BGNZ_DEBUG` | narrate requests on stderr (no credentials) |

## Exit codes

`0` fine, `1` failed, `2` misuse, `3` sign in, `5` no such store, `7` rate
limited.

## Where the code is

The storefront protocol is [`bgnz-api`](../../packages/bgnz-api); rendering is
[`cli-kit`](../../packages/cli-kit) and the process boundary is
[`net-kit`](../../packages/net-kit). `doctor`, `auth status` and the running
totals on a cart or an order come from `cli-kit`, so they are the same shape
here as in `fsnz`, `kmart` and the rest. What is left in `src/` is reading the
environment once, resolving flags against config, driving the browser, and
turning a failure into an exit code.
