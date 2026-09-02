# woolworths-nz-cli

Search Woolworths New Zealand from the terminal. A sibling of
[`foodstuffs-nz-cli`](../foodstuffs-nz-cli), built the same way, against the
other half of the New Zealand supermarket duopoly. It replaces
[`woolies-nz-cli`](https://github.com/mcinteerj/woolies-nz-cli).

> **Not affiliated with Woolworths New Zealand.** There is no public API. This
> calls the same undocumented GraphQL endpoint their website calls from the
> browser, and can break whenever they change something. Use at your own risk.

## Status

Verified live, end to end: `auth login`, `auth import`, `auth status`,
`stores`, `store set`, `search`, `specials`, `browse`, `departments`,
`doctor`, `orders list` and `cart list` — the last against a real cart of
nineteen products, weighed lines included.

`wwnz auth login` signs in through the Auth0 flow with no browser involved.
`wwnz auth import` remains the fallback if that is ever refused.

`orders show` does not exist. The capture these endpoints were traced from had
an empty order history, so the operation the website uses for one order's
contents never appeared in it, and guessing at its shape would be worse than
leaving it out. It could be captured now that there is a history to open.

Cart writes (`add`, `update`, `remove`, `clear`) are covered by tests against a
mock but have not been run against a live cart.

## Weighed lines

Loose produce and meat are sold by the kilogram — the `-KGM` variants — and
their quantities are **decimals in kilograms**, not counts. A cart line holding
300g of onions reads `0.3`, and 1.5kg of chicken nibbles reads `1.5`:

```bash
wwnz cart add 144329 0.3 --unit KGM      # 300g of brown onions
wwnz cart update 57133 1.5 --unit KGM    # 1.5kg of nibbles
```

Everything else is a count, and prints and serialises as a whole number.

The site's own `totalItemQuantity` does not work this way: it counts a weighed
line as one item however much of it there is, which is why the item count and
the sum of the quantities disagree. Both are reported as the site reports them.

## Getting past Akamai

Worth knowing, because it dictates the one unusual dependency.

woolworths.co.nz is behind Akamai Bot Manager, which scores the **TLS
handshake** — not the headers. With a stock `reqwest`/rustls client the
storefront withholds its bot-manager cookies (`ak_bmsc`, `bm_mi`, `bm_sv`) and
the login is refused with a bare `400` carrying no explanation. Matching the
browser's headers exactly changes nothing, and `curl` fares *worse* than rustls
— it is issued no `__guest__token` at all.

So this uses [`wreq`](https://github.com/0x676e67/wreq), which is `reqwest` with
a browser TLS and HTTP/2 fingerprint. One constant,
`session::EMULATION`, selects the profile and everything — handshake, HTTP/2
settings, `User-Agent` — is derived from it. Nothing sets headers by hand; a
header naming a different browser than the handshake is exactly the
inconsistency being watched for.

Akamai's JavaScript sensor never runs. The bot-manager cookies are issued on an
ordinary page load once the handshake looks right, so no browser is needed.

Note that the sibling `foodstuffs-nz-cli` solves the same *class* of problem
[the opposite way](../foodstuffs-nz-cli/README.md#getting-past-cloudflare),
shelling out to `curl` because Cloudflare accepts OpenSSL handshakes. Do not
copy that here — the two vendors score differently, and `curl` is the worse
client against this one.

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

Prices, specials and stock are per store. Pick one first.

```bash
wwnz stores whangarei                   # find a store
wwnz store set "Regent Woolworths"      # remember it
wwnz search milk
wwnz search milk --size 2L --limit 5
wwnz specials --limit 40
wwnz departments                        # what browse can select
wwnz browse "Fruit & Veg"
```

No sign-in is needed for any of that — a guest token is minted and cached on
first use. The cart and order history do need an account:

```bash
wwnz auth login --email you@example.com
wwnz auth status                        # does the session still work?
wwnz cart add 282768 2
wwnz cart list
wwnz orders list
wwnz orders previous                    # the site's "buy it again"
```

If the sign-in page ever refuses this client, export cookies from a browser and
`wwnz auth import cookies.txt` instead. Sessions last about 24 hours.

Everything takes `--json`.

## Commands

| Command | What it does |
| --- | --- |
| `search <query>` | Product search. `--specials` narrows it to what is on offer |
| `specials` | Everything on special at the selected store |
| `browse <department>` | A whole department, aisle or shelf |
| `departments [query]` | The tree `browse` selects from, with the keys |
| `stores [query]` | Stores by name, suburb or address |
| `store show\|set\|clear` | The store prices are quoted against |
| `cart list\|add\|update\|remove\|clear` | The shopping cart |
| `orders list` | Past and current orders |
| `orders previous` | What this account buys regularly |
| `auth login\|import\|logout\|status` | Signing in, and checking a session |
| `doctor` | Config, credentials and connectivity |
| `completions <shell>` | A shell completion script |
| `update` | Check for and install a newer release |

## How it works

Everything goes to one endpoint, `POST /api/graphql`. Authorisation is entirely
by cookie, and there are two kinds:

- **`__guest__token`** — a JWT the storefront hands to anyone who loads the home
  page. Enough for products, departments and stores. It is minted on demand and
  cached under the state directory until it expires.
- **`__session__0` / `__session__1`** — an encrypted session cookie, split in two
  because it is too large for one. Only Woolworths' own server can mint or read
  it, so there is no token endpoint to call: `wwnz auth login` walks the same
  Auth0 redirect chain a browser does and keeps what falls out. The session is a
  credential and goes to the system keychain (or a 0600 file where there is no
  keychain).

Two things about this API shape the tool:

**The store is a property of the cart, not a search parameter.** Selecting a
store is a `SetCartShoppingMode` mutation, and it is what the prices in a
subsequent search are keyed to. So `wwnz store set` binds it *and* saves it, and
every product command re-binds before searching.

**Search, browse, specials and buy-again are one operation.** They are four
fields of a single `CompositeSearchInput`, which is why one GraphQL document and
one `SearchBy` enum cover all of them.

The documents in [`src/api/gql.rs`](src/api/gql.rs) are cut down from what the
website sends — it asks for everything its React tree might render, and its
search document alone is about five kilobytes. Asking only for the fields this
tool prints keeps the requests small and means a field they add elsewhere cannot
break a query here.

## When it breaks

It will. These are undocumented endpoints behind Akamai bot management.

- **`wwnz doctor`** first. It checks the guest token, the API, the store and the
  account separately, so it says *which* half is broken.
- **The endpoints move**: `WWNZ_ORIGIN` and `WWNZ_AUTH_ORIGIN` override them, so
  you can follow without waiting for a release.
- **The storefront serves a bot check** instead of a guest token: `doctor` says
  so outright. `WWNZ_GUEST_TOKEN` takes one lifted from a browser.
- **The session lapses** — `session_expired` on any account command. Re-export
  the cookies and `wwnz auth import` again. `WWNZ_SESSION` takes a `Cookie`
  header value directly, for a one-off.
- **`auth login` returns 400** at the email step. Akamai is refusing the
  client again; `WWNZ_DEBUG_AUTH=1` shows where it stops. Try a different
  `session::EMULATION` profile, or fall back to `auth import`.

## Environment

| Variable | Effect |
| --- | --- |
| `WWNZ_STORE_ID` | Store to price against |
| `WWNZ_SESSION` | A `Cookie` header value, overriding the stored session |
| `WWNZ_GUEST_TOKEN` | A guest token, instead of minting one |
| `WWNZ_EMAIL` | Default for `auth login --email` |
| `WWNZ_ORIGIN` | The storefront and API origin |
| `WWNZ_AUTH_ORIGIN` | Where the login flow is served |
| `WWNZ_CONFIG_DIR` | Config directory |
| `WWNZ_STATE_DIR` | State directory (guest token, install marker) |
| `WWNZ_SECRET_BACKEND` | `keyring` or `file` |
| `WWNZ_DEBUG_AUTH` | Narrate each step of the sign-in flow to stderr |

## Config

`~/.config/woolworths-nz-cli/config.toml`:

```toml
store_id = "9048"
store_name = "Regent Woolworths"
# Optional: keeps the password out of this file and out of shell history.
password_command = "pass show woolworths"
```

## Development

```bash
dispat run check --since all -p woolworths-nz-cli   # what CI runs
dispat run test  --since all -p woolworths-nz-cli
```

Tests run against a `wiremock` stand-in for the API, routing on the `op-name`
query parameter the way the real endpoint distinguishes operations. Nothing in
the test suite touches the network.
