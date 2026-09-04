# fsnz-api

The Foodstuffs NZ edge API: New World and PAK'nSAVE, plus the Club Plus login
in front of them.

Both banners are one Foodstuffs platform wearing two names, so one client drives
both. They differ in which hostnames they answer on and which code their tokens
are scoped to, and that is what [`Banner`](src/banner.rs) carries.

> Reverse-engineered from what the storefronts' own frontends call. There is no
> public API and no documentation; these endpoints can change without notice.

## Vendor-shaped on purpose

This crate speaks Foodstuffs' vocabulary and depends on no shared domain crate.
Converting to [`gsnz-core`](../gsnz-core) is the caller's job — the adapter
lives in the app — which is what keeps this usable on its own and keeps a
Foodstuffs quirk from leaking into a type Woolworths also has to fit.

Every field arrives optional, deliberately: a field Foodstuffs renames should
degrade to a missing column, not a failed command.

## What is in it

| Module | What it does |
| ------ | ------------ |
| [`client`](src/client.rs) | The edge API: paged search (which is also specials and browse), stores, categories, cart, orders |
| [`token`](src/token.rs) | Getting a bearer token, cheapest source first, and caching it |
| [`auth`](src/auth/) | The Club Plus login, the device identity it insists on, and session renewal |
| [`banner`](src/banner.rs) | The two banners and where each answers |
| [`cart`](src/cart.rs) / [`order`](src/order.rs) | The cart and past orders. Money in cents; a line's price is the line total |
| [`wire`](src/wire.rs) | The shapes Foodstuffs actually sends, and how they become the types above |
| [`http`](src/http.rs) | The emulation profile, the cookie filter, and the client spec |

```rust
use fsnz_api::{Banner, Client, DEFAULT_SORT};

let jar = std::sync::Arc::new(net_kit::Jar::load(&secrets, "newworld", fsnz_api::cookie_keep));
let http = net_kit::http::build(fsnz_api::client_spec(jar))?;
let client = Client::new(http, Banner::NewWorld, endpoints, token);

// One search shape: the store, the query, and a filter string that scopes it.
let filters = fsnz_api::filters(store_id, specials_only, department);
let found = client.collect(store_id, "milk", &filters, 20, DEFAULT_SORT).await?;
```

`collect` pages until it has what was asked for or the results run out;
`filters` builds the Algolia-style filter that scopes a search to a store, to
what is on promotion there, or to a department.

## Two kinds of token, cached apart

The read APIs need a bearer token but not an account: loading the storefront
sets an `fs-user-token` cookie holding a short-lived JWT, and that JWT
authorises search, specials and stores. An account token is a different thing,
minted through Club Plus.

They are cached separately because they authorise different endpoints —
`/v1/edge/store` answers a guest token with the store list and an account token
with a flat 400. Tokens are also scoped to one banner: a New World token
presented with a PAK'nSAVE store does not fail, it answers the cart endpoints
with an empty cart belonging to nobody.

## The Club Plus login, and the step that must go to Club Plus

No browser is involved. Three calls:

1. `login.clubplus.co.nz/api/apigee-credentials` hands a bearer token to anyone
   who asks; it is the key for the login API itself.
2. `POST .../user/login` exchanges email and password for a Club Plus session —
   or, from an unrecognised device, for a code emailed to the account, redeemed
   at `POST .../user/tfa/login`.
3. `POST {clubplus api}/user/token/secure` issues a single-use code scoped to
   one banner, and the storefront's `/api/user/login/sso` swaps it for the
   `fs-user-token` that banner's API wants.

**Step 3 has to go to Club Plus.** The banner API answers the same path with a
200 and a plausible `secure_token`, but the code it issues ignores the `banner`
field and exchanges back into a national (`NAT`) token. Nothing fails; the cart
endpoints just quietly answer with an empty cart belonging to nobody. Only the
Club Plus code exchanges into `MNW`/`PNS`.

The session renews itself from a rotating refresh token, so the replacement is
written to the credential store *before* the session is used — losing it is what
ends a session, and a refresh token spent elsewhere invalidates the stored one.

## Development

```bash
dispat run check --since all -p fsnz-api
```

Unit tests beside the code for the wire shapes and the token logic;
[`tests/`](tests) drives the client and the login chain against a `wiremock`
stand-in. Nothing in the suite touches the network — the one thing that cannot
be covered that way is a real Club Plus login, which is verified by hand.

Used by [`grocery-nz-cli`](../../apps/grocery-nz-cli) and
[`foodstuffs-nz-cli`](../../apps/foodstuffs-nz-cli). Not published to
crates.io; consumers declare a path dependency, as
[`packages/README.md`](../README.md) describes.
