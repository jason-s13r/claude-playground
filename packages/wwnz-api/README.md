# wwnz-api

The Woolworths NZ GraphQL API, and the Auth0 login flow in front of it.

One endpoint, `POST /api/graphql`, answers everything the website does: search,
browse, specials, stores, the cart and order history.

> Reverse-engineered from the site's own traffic. There is no public API and no
> documentation; these endpoints can change without notice.

## Vendor-shaped on purpose

This crate speaks Woolworths' vocabulary and depends on no shared domain crate.
Converting to [`gsnz-core`](../gsnz-core) is the caller's job — the adapter
lives in the app — which is what keeps this usable on its own.

Everything arrives optional, deliberately: a field Woolworths renames should
degrade to a missing column, not a failed command.

## What is in it

| Module | What it does |
| ------ | ------------ |
| [`client`](src/client.rs) | The one endpoint, and everything reached through it |
| [`gql`](src/gql.rs) | The GraphQL documents, cut down to the fields this crate returns |
| [`session`](src/session.rs) | The guest token and the account session, and which calls need which |
| [`auth`](src/auth.rs) | Walking the Auth0 redirect chain to obtain a session |
| [`wire`](src/wire.rs) | The shapes the API answers with, and how they become domain types |
| [`endpoints`](src/endpoints.rs) | Where it all lives — plain fields, so a test can point the flow at a mock |
| [`error`](src/error.rs) | What Woolworths said no with, read off the GraphQL extension code |

```rust
use wwnz_api::{Client, Endpoints, SearchBy, Session, DEFAULT_SORT};

let http = net_kit::http::build(wwnz_api::client_spec())?;
let client = Client::new(http, Endpoints::default(), Session::guest(token));

// No store argument: the store is a property of the cart, bound server-side.
let found = client.search(&SearchBy::Keyword("milk".into()), 20, DEFAULT_SORT, false).await?;
```

**The store is not a search parameter.** Selecting one is a
`SetCartShoppingMode` mutation, and it is what the prices in a subsequent search
are keyed to — so `set_store` binds it, and a caller re-binds before searching.

## Authorisation is entirely by cookie

Two kinds:

- **`__guest__token`** — a JWT the storefront hands to anyone who loads the home
  page. Enough for products, departments and stores.
- **`__session__0` / `__session__1`** — an encrypted session cookie, split in two
  because it is too large for one. Needed for the cart and orders.

Only Woolworths' own server can mint or read the session cookie, so there is no
token endpoint to call and **no way to refresh it**. The only renewal is walking
the whole login flow again, which needs the password — which is why a caller
that wants to survive a lapsed session unattended has to keep one.

## The login flow

```text
  GET  www  /auth/login          -> 307 to auth /authorize
  GET  auth /authorize           -> 302 to /u/login/identifier?state=...
  POST auth /u/login/identifier  -> 302 to /u/login/password?state=...
  POST auth /u/login/password    -> 302 to /authorize/resume
  GET  auth /authorize/resume    -> 302 to www /auth/callback?code=...
  GET  www  /auth/callback       -> 307, and sets __session__0/__session__1
```

Two things make this tractable. The storefront starts the flow itself, so the
PKCE challenge, `state` and `nonce` are generated server-side and never have to
be computed here. And each Auth0 form echoes the flow's `state` in a hidden
field, so following it is a matter of scraping one value and posting it back.

Getting through the Akamai bot management in front of the auth host is entirely
a matter of the TLS handshake. With a stock rustls client the identifier step is
answered with a bare `400` and no explanation; with the browser handshake
[`EMULATION`](src/http.rs) selects, the same requests are answered normally.
That is also why this client is built with **no cookie jar and no redirect
policy** — the opposite of the Foodstuffs one, and not interchangeable with it:
an unexpected redirect here is a bot check, and it has to surface rather than be
quietly followed.

It is still somebody else's login page, and a change to it will break this. A
session can also be filled in from cookies exported from a browser, which is the
way back in when that happens.

## The documents are cut down

The site asks for everything its React tree might render — ad slots, roundel
artwork, health star ratings — and its `ProductSearch` document alone is about
five kilobytes. Asking only for the fields this crate returns keeps the requests
small and, more usefully, means a field the site adds or renames elsewhere
cannot break a query here.

Operation names are kept exactly as the site spells them. They travel in the
`wnzx-operation-name` header and the `op-name` query parameter as well as in the
document, and there is no reason to look like a different client.

Search, browse, specials and buy-again are four fields of one
`CompositeSearchInput`, which is why a single document and one `SearchBy` enum
cover all of them.

## Prices arrive two ways

Search quotes dollars as a JSON number (`7.19`); the cart and orders quote whole
cents (`719`). Both are converted at the boundary and nothing keeps
floating-point money past it.

## Development

```bash
dispat run check --since all -p wwnz-api
```

Unit tests beside the code, and [`tests/client.rs`](tests/client.rs) against a
`wiremock` stand-in that routes on the `op-name` query parameter the way the
real endpoint distinguishes operations. Nothing touches the network.

Used by [`grocery-nz-cli`](../../apps/grocery-nz-cli). Not published to
crates.io; consumers declare a path dependency, as
[`packages/README.md`](../README.md) describes.
