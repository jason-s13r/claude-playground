# kmart-cli

`kmart` — Kmart from the command line, in Australia and New Zealand.

```bash
kmart search "milk frother"
kmart use au                       # switch country; prices, stock and catalogue all follow
kmart product 43165537
kmart stock 43165537
kmart stores
```

Unofficial, and built by reading the site's own traffic. See
[`kmart-api`](../../packages/kmart-api) for what that traffic looks like.

## Two countries

Kmart runs one backend for both. `kmart use nz` and `kmart use au` switch which
one every command asks, and `--country au` does it for a single command without
saving. It is not a display setting: each country has its own catalogue at its
own prices in its own currency.

Store ids are shared across the pair, so `kmart stores 8229` describes the same
shop whichever country is selected.

## Searching works out of the box. Stock needs a browser once.

The catalogue — `search`, `browse`, `categories`, `product` — runs against
Kmart's search index, which needs no account and no setup. Install the binary
and it works.

Everything else goes through Kmart's own gateway, which sits behind Akamai Bot
Manager. That check cannot be passed by a command line tool: it wants a payload
computed by a script it ships to browsers. So the cookies come from a browser
that already passed it:

```bash
# with kmart.co.nz or kmart.com.au open and signed in, export cookies.txt
kmart auth import ~/Downloads/cookies.txt
```

They last about a day. `kmart doctor` says whether the ones you have are still
good, and reports the catalogue and the gateway separately so a missing import
does not look like an outage.

Signing in is the other half. Kmart's bot check guards the one step of Auth0's
login that mints a session — the password submit — and refuses any plain HTTP
client. So `kmart auth login` drives a real browser to do it:

```bash
kmart auth login --email you@example.com
kmart auth login --email "$(op read 'op://Vault/Kmart/email')" \
                 --password-command 'op read "op://Vault/Kmart/password"'
```

It runs headless by default; add `--headful` to watch the window, which is also
the stronger path when the bot check is being stubborn. **One run earns both
credentials**, the token and the cookies, so it replaces `auth import` as well.

That needs [camoufox](https://camoufox.com) installed separately, because a
browser engine is a hundred megabytes and most of this tool does not need one:

```bash
uv tool install "camoufox[geoip]" && camoufox fetch
```

`kmart auth login` says the same if it cannot find camoufox on your `PATH`.

Without a browser, both halves can still be fetched by hand from a signed-in
tab — `kmart auth import <cookies.txt>` for the cookies, and for the token:

```bash
# devtools → Local Storage → the key beginning @@auth0spajs@@ → refresh_token
kmart auth token          # prompts, so it stays out of your shell history
```

Give it no argument and it prompts: a refresh token passed on the command line
ends up in your shell history. It is spent once on the spot to check it, so a
placeholder or a half-copied token is refused there and then rather than
failing later somewhere that looks like a Kmart problem.

**Copy it and import it in the same minute.** Auth0 rotates refresh tokens for
browser apps — every use invalidates the one before — so a token copied an hour
ago has already been spent by the tab you copied it from, and will be refused.
That is what a rejection almost always means, rather than a bad paste.

Once imported it *is* a one-off — `kmart` keeps each rotated token as it goes,
so it renews indefinitely. Unlike the cookies, which expire daily. The flip
side of rotation is that the browser's copy goes stale once `kmart` starts
renewing, so that tab will eventually ask you to sign in again; signing in
there breaks nothing.

The two credentials are independent, and `kmart auth status` says so: you can
have a token without cookies (nothing will work) or cookies without a token
(stock and stores will).

## Stock is a postcode question

Not a store question. Set one first:

```bash
kmart postcode set 1010
kmart stock 43165537
```

In New Zealand that also sets the island, because the postcode already knows
it. Kmart ranges differently across the strait, so `kmart island` changes what
a listing *contains*, not how it is shown. Australia has no equivalent.

Stock comes back per channel — home delivery, click and collect, express —
because an item can be sold out online and on a shelf a suburb away.

## Keycodes

A product is a keycode: `43165537`. The same id works in every command and on
the website's own URLs, which end in it.

A product with sizes or colours is not a leaf — each variation has a keycode of
its own, and that is what goes in a cart. `kmart product` lists them when there
is more than one.

## Refinements differ per category

There is no fixed list of filters, because Kmart's facets are per category —
`Material` on a bucket, `Power Rating` on an appliance, `Book Genre` on a
novel. So you ask:

```bash
kmart browse "Mops" --facets
kmart browse "Mops" --filter 'Material=Polypropylene' --filter 'Colour=Multi'
```

`--brand`, `--color` and `--size` are shortcuts for the three that are always
there.

## Everything else

```bash
kmart cart                         # needs an account
kmart cart add 43165537 2
kmart wishlist
kmart orders
kmart config list
kmart completions zsh
kmart update
```

`--json` on any command prints a document instead of a table, from the same
struct the table is rendered from.

`kmart wishlist` reads and adds. It cannot remove — the operation Kmart's own
site uses for that was never captured, and the gateway has introspection turned
off, so its name cannot be discovered. Guessing one would fail at runtime
rather than at build time. See the `kmart-api` README.

## Exit codes

So a script can tell failures apart without reading the message:

| | |
| --- | --- |
| `2` | bad flags or a bad config value |
| `3` | not signed in, or the session lapsed |
| `5` | no such store |
| `6` | no such product or category |
| `7` | rate limited — worth waiting and retrying |
| `8` | the bot check — **not** worth retrying; import cookies |

`7` and `8` are apart on purpose. Waiting clears a rate limit; nothing clears a
bot challenge except a browser.

## Configuration

`kmart config list` shows every setting. The file is at
`~/.config/kmart-cli/config.toml` (`kmart config path`), and every setting has
a flag or an environment variable that beats it.

`KMART_CONFIG_DIR`, `KMART_STATE_DIR`, `KMART_SECRET_BACKEND`, `KMART_COUNTRY`,
`KMART_DEBUG`, and `KMART_API` / `KMART_SEARCH` / `KMART_AUTH` for pointing the
binary at a mock server.

The state directory also holds `vendor-nz.json` and `vendor-au.json`: the
search key and Auth0 client id that country's storefront is currently serving,
re-read about once a week. They are Kmart's to rotate, and this is what makes
a rotation heal itself rather than needing a new release. Deleting one costs a
single request. If the storefront cannot be reached the values compiled in are
used instead, so nothing here can stop a search working — `kmart doctor` says
which of the two is in force under `front end`.

## Building

```bash
dispat run check --since all -p kmart-cli
dispat run run --since all -p kmart-cli -- search mop
```
