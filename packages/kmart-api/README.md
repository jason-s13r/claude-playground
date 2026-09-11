# kmart-api

Kmart, in Australia and New Zealand: the catalogue, the stock, the cart, the
wishlist and the login. Reverse-engineered from the site's own traffic, so
everything here is undocumented and everything arrives optional.

Vendor-shaped types, no shared domain crate — nothing above this is a grocery,
so `Product` and `Store` here are the final types.

## Two countries, one backend

`kmart.com.au` and `kmart.co.nz` are the same service: the same GraphQL schema
answers on both hosts, store ids share a namespace across the pair, and the
login is a single Auth0 tenant, Australian-domained, for both.

What differs is the catalogue, the money, and — measured, not assumed — the
Auth0 *application*: one tenant, but a separate registered client per
storefront. A refresh token is bound to the client that minted it, so
[`StoredSession`](src/session.rs) records which storefront signed in and a
renewal uses that one whatever country the command is asking about. So
[`Country`](src/country.rs) is a parameter threaded through the crate, not a
build of its own.

## The vendor's own identifiers

The Constructor index key and the Auth0 client id are Kmart's to rotate.
[`vendor`](src/vendor.rs) is the single place they live, and
[`Vendor::read`](src/vendor.rs) pulls a current set out of a storefront home
page — one of the few Kmart URLs a plain HTTP client is served. The caller
decides which to use; the CLI prefers what it read, caches it for a week, and
falls back to what shipped.

None of them is a secret — every visitor's browser downloads all of them — but
they look exactly like leaked credentials, so that one file is named in
`.github/secret_scanning.yml`. The price of the exemption is that nothing which
*is* secret may go in it.

## Four surfaces

| Surface | Host | Carries |
| --- | --- | --- |
| Constructor.io | `ac.cnstrc.com` | search, browse, categories, facets, one product's full record |
| GraphQL gateway | `api.kmart.{com.au,co.nz}` | stock, stores, postcodes, cart, wishlist, orders |
| Auth0 | `auth.kmart.com.au` | the login, and the tokens the gateway wants |
| storefront | `www.kmart.{com.au,co.nz}` | nothing this crate needs |

The storefront is listed only to say it is not used: its product pages carry
the whole record in a `__NEXT_DATA__` blob, but that record *is* the
Constructor one, and the storefront is bot-checked while Constructor is not.
This crate parses no HTML except the Auth0 login form.

## What needs what

| | needs a token | needs cookies |
| --- | --- | --- |
| search, browse, categories, product | no | no |
| postcodes, stock, stores | no | **yes** |
| cart, wishlist, orders, customer | **yes** | **yes** |

**The catalogue works cold.** Constructor.io is a third party serving Kmart's
feed on a key the storefront ships in its own JavaScript.

**The gateway does not.** Akamai Bot Manager sits in front of it and answers a
client that has not run its sensor script with a `429` carrying `cpr_chlge`, or
a `200` whose body is an interstitial. Measured against the live site: this
happens on every emulation profile tried — Firefox 139 and 151, Chrome 142,
Safari 26 — with a cookie jar, with a home-page warm-up, and with the CORS
preflight a browser sends first.

So gateway calls are admitted by cookies exported from a browser that already
passed the check, via [`auth::from_netscape`](src/auth.rs), which is why
`_abck` is treated as a credential. They last about a day.

**The login is guarded at exactly one step.** Measured against the live site:

| step | answer |
| --- | --- |
| `GET /authorize` | the form |
| `POST /u/login/identifier` | the next form |
| **`POST /u/login/password`** | **`403`, Akamai "Access Denied"** |
| `POST /oauth/token` | Auth0, for real |

Its refusal is a `403` with an HTML body, indistinguishable from a wrong
password unless the body is read — so it is read, and `Error::Challenged` comes
back rather than `LoginRefused`.

`auth::login` is kept regardless: it is correct against Auth0, one policy
change away from working, and the executable description of the flow.

## Signing in

The way in that works is `auth::refresh`, driven by a refresh token lifted out
of a browser once. The token endpoint is **not** guarded, so from there a
session renews itself indefinitely — unlike the cookies beside it, which expire
daily.

`auth::login` walks Auth0's hosted login with two wrinkles the Woolworths flow
does not have. Kmart's storefront is a single-page app, so it generates its own
PKCE pair rather than having a server do it — meaning this crate does, and the
RFC 7636 worked example is a unit test. And the flow ends at a `code` on a
redirect *to the storefront*, which is bot-checked: the redirect is read, never
followed, or the code is traded for a challenge page.

Auth0 may interrupt a correct password with an offer to enrol a passkey. There
is no way to pre-decline it in the authorize request, so it is declined when it
appears.

`offline_access` means the fifteen-minute access token renews from a refresh
token — one POST, no password. `Client::renew` goes token, then refresh grant,
then the whole login, and most runs stop at the first. The last of those is the
step Akamai blocks, so a session that has lost its refresh token is over.

## Identifiers

A product's leaf id is its **keycode** — `43165537`. It is Constructor's
`variation_id`, the gateway's `sku`, and the last segment of the page slug, so
one id works everywhere and there is no translation anywhere in this crate.

A product is not a leaf: a t-shirt's twenty-four sizes each carry a keycode of
their own, and it is those the cart takes. A category is a 32-hex group id.

## Stock is a function of a postcode

Not of a store. The gateway is asked what is available near a postcode and
answers per channel — home delivery, click and collect, express, in store —
and the channels are independent, which is why `Availability` is not a boolean.

The four channels are shaped differently on the wire: the two that ship have
one pooled `stock`, the two that involve walking into a building have
`locations`. Asking for `stock` on `IN_STORE` is a schema error, not an empty
answer.

There is no store-finder endpoint. `stores_near` asks about a sentinel keycode
and keeps the geography — the collect network for that postcode, ordered by
distance — then names each store with a call of its own.

## Known gap: taking something off the wishlist

`wishlist` and `wishlist_add` work. Removing an item and changing its quantity
do not exist here: the site's own removal was never captured in a HAR, and
introspection is disabled on the gateway —

```
GraphQL introspection is not allowed by Apollo Server
```

— so the operation name cannot be discovered. One capture of a removal in a
browser is all it needs.

## Loose parsing

Nothing is required, and unfamiliar fields survive in `Product::extra` rather
than being dropped. Two shapes worth knowing about:

- A store's `latitude` arrives as the string `"-36.914257999999997"` while
  `distanceInKm` in the same response is the number `16.6`.
- An order total is dollars; a cart total is cents. Same gateway.

A term matching nothing does not return nothing: the index falls back to a
semantic match, so gibberish still answers with twenty products. `Listing::exact`
is carried separately from `total`, and `Listing::is_guess` says so.

## Tests

```bash
dispat run check --since all -p kmart-api
```

Unit tests run against fixtures cut from live responses; the integration suite
runs against `wiremock` with all four hosts pointed at one mock server, which
is what `Endpoints::all` is for. No fixture carries a credential.
