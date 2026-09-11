# kmart-cli

`kmart` — Kmart from the command line, in Australia and New Zealand.

```bash
kmart search "milk frother"
kmart use nz                       # switch country; prices, stock and catalogue all follow
kmart product 43165537
kmart stock 43165537
kmart stores
```

Unofficial, and built by reading the site's own traffic. See
[`kmart-api`](../../packages/kmart-api) for what that traffic looks like.

## Two countries

Kmart runs one backend for both. Australia is the default; `kmart use nz` and
`kmart use au` switch which one every command asks, and `--country nz` does it
for a single command without saving. Each country has its own catalogue at its
own prices in its own currency.

`kmart auth login` is the exception: it records the country it signed in to,
because a session belongs to the storefront that minted it.

Store ids are shared across the pair, so `kmart stores 8229` describes the same
shop whichever country is selected.

## Searching works cold; stock needs a browser once

The catalogue — `search`, `browse`, `categories`, `product` — runs against
Kmart's search index, which needs no account and no setup.

Everything else goes through Kmart's gateway, which sits behind Akamai Bot
Manager. That check wants a payload computed by a script it ships to browsers,
so it cannot be passed by a command line tool. The cookies come from a browser
that already passed it:

```bash
# with kmart.co.nz or kmart.com.au open and signed in, export cookies.txt
kmart auth import ~/Downloads/cookies.txt
```

They last about a day. `kmart doctor` says whether the ones you have are still
good, and reports the catalogue and the gateway separately.

Signing in is the other half. The bot check guards the one step of Auth0's
login that mints a session — the password submit — so `kmart auth login` drives
a real browser to do it:

```bash
kmart auth login --email you@example.com
kmart auth login --email "$(op read 'op://Vault/Kmart/email')" \
                 --password-command 'op read "op://Vault/Kmart/password"'
```

Headless by default; `--headful` watches the window and is the stronger path
when the bot check is being stubborn. **One run earns both credentials**, the
token and the cookies, so it replaces `auth import` as well.

It needs [camoufox](https://camoufox.com), installed separately because a
browser engine is a hundred megabytes:

```bash
uv tool install "camoufox[geoip]" && camoufox fetch
```

Without a browser, both halves can be fetched by hand from a signed-in tab —
`kmart auth import <cookies.txt>` for the cookies, and for the token:

```bash
# devtools → Local Storage → the key beginning @@auth0spajs@@ → refresh_token
kmart auth token          # prompts, so it stays out of your shell history
```

The token is spent once on the spot to check it, so a placeholder or a
half-copied one is refused there and then.

**Copy it and import it in the same minute.** Auth0 rotates refresh tokens for
browser apps — every use invalidates the one before — so a token copied an hour
ago has already been spent by the tab you copied it from. That is what a
rejection almost always means, rather than a bad paste.

Once imported it is a one-off: `kmart` keeps each rotated token as it goes, so
it renews indefinitely, unlike the cookies. The browser's copy goes stale in
return, so that tab will eventually ask you to sign in again; signing in there
breaks nothing.

The two credentials are independent, and `kmart auth status` says so: a token
without cookies gets you nothing, cookies without a token get you stock and
stores.

### Unattended

`kmart auth refresh` is `auth login` without the typing:

```bash
kmart auth refresh          # exit 0 if the session is good, 3 if it needs you
```

It renews the cheap half first: a good refresh token costs one request to an
endpoint Akamai does not guard, and most runs stop there. The cookies have no
clock to read — a stale `_abck` looks exactly like a good one — so it tests
them by spending a single gateway request, and only a refusal opens a browser.

The password it signs in with is `auth.password_command` where you set one,
otherwise the copy `auth login` kept. With neither, a refused token exits 3.

```bash
kmart config set auth.password_command 'op read "op://Vault/Kmart/password"'
```

`--force` renews both halves whatever state they are in. Not the default: Auth0
rotates the refresh token on every use.

## Stock is a postcode question

Not a store question. Set one first:

```bash
kmart postcode set 1010
kmart stock 43165537
```

In New Zealand that also sets the island, because the postcode knows it. Kmart
ranges differently across the strait, so `kmart island` changes what a listing
*contains*. Australia has no equivalent.

Stock comes back per channel — home delivery, click and collect, express —
because an item can be sold out online and on a shelf a suburb away.

## Keycodes

A product is a keycode: `43165537`. The same id works in every command and ends
the website's own URLs.

A product with sizes or colours is not a leaf — each variation has a keycode of
its own, and that is what goes in a cart. `kmart product` lists them when there
is more than one.

## Refinements differ per category

Kmart's facets are per category — `Material` on a bucket, `Power Rating` on an
appliance, `Book Genre` on a novel — so there is no fixed list of filters:

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

`kmart wishlist` reads and adds. It cannot remove: the operation Kmart's own
site uses was never captured, and the gateway has introspection turned off, so
its name cannot be discovered. See the `kmart-api` README.

## Exit codes

| | |
| --- | --- |
| `2` | bad flags or a bad config value |
| `3` | not signed in, or the session lapsed |
| `5` | no such store |
| `6` | no such product or category |
| `7` | rate limited — worth waiting and retrying |
| `8` | the bot check — **not** worth retrying; import cookies |

Waiting clears a rate limit; nothing clears a bot challenge except a browser.

## Configuration

`kmart config list` shows every setting. The file is at
`~/.config/kmart-cli/config.toml` (`kmart config path`), and every setting has
a flag or an environment variable that beats it.

`KMART_CONFIG_DIR`, `KMART_STATE_DIR`, `KMART_SECRET_BACKEND`, `KMART_COUNTRY`,
`KMART_DEBUG`, and `KMART_API` / `KMART_SEARCH` / `KMART_AUTH` for pointing the
binary at a mock server.

The state directory also holds `vendor-nz.json` and `vendor-au.json`: the
search key and Auth0 client id that country's storefront is currently serving,
re-read about once a week. They are Kmart's to rotate, so re-reading them makes
a rotation heal itself. Deleting one costs a single request; if the storefront
cannot be reached the values compiled in are used instead. `kmart doctor` says
which of the two is in force under `front end`.

## Building

```bash
dispat run check --since all -p kmart-cli
dispat run run --since all -p kmart-cli -- search mop
```
