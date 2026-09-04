# twlnz-api

The Warehouse NZ storefront: its listings, products, cart, wishlist and stores.

Salesforce Commerce Cloud (Demandware), not an API. Cart, stores, variations and
the wishlist's *writes* answer clean JSON, but **everything that lists products
is server-rendered HTML** — the wishlist included — and in a 1,195-request
capture of the site, 18 responses were JSON.

> Reverse-engineered from the site's own traffic. There is no public API and no
> documentation; these endpoints can change without notice.

## Vendor-shaped on purpose

This crate speaks The Warehouse's vocabulary and depends on no shared domain
crate. It is deliberately *not* built on [`gsnz-core`](../gsnz-core): that is a
grocery domain, and this is general merchandise that happens to sell food —
there is nowhere in a `SaleUnit` to put a colour axis.

Everything arrives optional, deliberately: a field The Warehouse renames should
degrade to a missing column, not a failed command.

## What is in it

| Module | What it does |
| ------ | ------------ |
| [`client`](src/client.rs) | The storefront, and everything reached through it |
| [`extract`](src/extract.rs) | **The only module that parses HTML.** Tiles, page tokens, JSON-LD, stock markup, wishlist rows |
| [`listing`](src/listing.rs) | Search and browse, which are one endpoint, and how paging works |
| [`product`](src/product.rs) | `Pdp` — a product page that has been read, and what it authorises |
| [`cart`](src/cart.rs) | The basket and the wishlist, and reading a refusal off a 200 |
| [`stores`](src/stores.rs) | The store finder, and per-store stock |
| [`auth`](src/auth.rs) | The form login |
| [`session`](src/session.rs) | The cookies, and telling a signed-in shopper from a guest |
| [`wire`](src/wire.rs) | The JSON shapes, and how they become domain types |
| [`endpoints`](src/endpoints.rs) | Where it all lives — plain fields, so a test can point the flow at a mock |
| [`error`](src/error.rs) | What The Warehouse said no with |

```rust
use twlnz_api::{Client, Endpoints, Query, Session};

let http = net_kit::http::build(twlnz_api::client_spec())?;
let client = Client::new(http, Endpoints::default(), Session::default());

let listing = client.search(&Query::Keyword("lego".into()), 20, None, &[]).await?;

// A write is a two-step: read the page, then spend the token it carries.
let pdp = client.pdp("R3059518").await?;
let cart = client.add_to_cart(&pdp, 1).await?;
```

## Five things that will bite

Each of these was found by running against the live site, and each one is the
difference between working and not.

### 1. The emulation profile is load-bearing

Cloudflare sits in front of this storefront. Measured, with everything else held
constant: `Firefox151` and `Safari26_4` are served the page; `Firefox149` and
`Chrome149` are answered with a **403 managed challenge on the home page
itself**. An older profile does not degrade — it stops working entirely. See
[`http`](src/http.rs).

### 2. `Sec-Fetch-*` decides whether an action is allowed

The cart, wishlist, variation and stock endpoints answer

```json
{"error": true, "errorMessage": "Cross-Origin Request Blocked"}
```

with a 403 unless the request carries `Sec-Fetch-Dest: empty`,
`Sec-Fetch-Mode: same-origin` and `Sec-Fetch-Site: same-origin`. The emulation
sends the *navigation* values, which is exactly what this rejects — so a request
with the right token and the right cookies is refused for its headers alone.

**The message means what it says.** It is not a disguised complaint about the
token, and treating it as one buys a wasted page fetch and an identical failure.

### 3. Writes are a two-step, and stock is a three-step

Cart, wishlist, variation and stock all need `verify=<unixtime>-<base64 HMAC>`,
minted into the product page. Nothing takes one that was built rather than
scraped. [`Pdp`](src/product.rs) is that two-step made into a type: the
token-bearing operations take a `Pdp`, so the page fetch is something the caller
did rather than something hidden inside a call.

Narrowing stock to a region is a **third** request: the stock modal renders one
pre-signed URL per region, each with its own token, and the product page's token
does not authorise the regional endpoint.

### 4. The login redirect must not be followed

`POST /account/submit-login?rurl=1` answers **302, and every cookie that matters
is set on the 302 itself**. This crate keeps no cookie jar, so letting the
client follow the redirect drops all of them and lands on `/account` as a guest.

The failure is quiet and looks exactly like a rejected password, which is the
worst part: the request succeeded, the redirect succeeded, and the session
simply is not there. So [`auth`](src/auth.rs) overrides the client's redirect
policy for that one request rather than relying on it, and reads the cookies off
the 302.

A refused password is a **200** — the sign-in page rendered again, with the
reason in it. So the status alone tells success from failure, and the page
carries the words worth repeating.

### 5. A product id is not a leaf

`RM110166766` is a variation master; `RM110166766-10M` is a variation group and
`R3043978` one of its variants — and the cart takes a variant. A listing tile
links to one thing and its tracking payload names another, so
[`extract`](src/extract.rs) prefers the link.

Availability is **two-dimensional**, not a boolean. One observed variant had no
online stock and was orderable in a shop (`productStatus: "FIND_IN_STORE"`), so
`Availability` carries both axes and `summary()` says `in store` rather than
`sold out`.

## Reading the HTML

Every product tile carries a `data-gtm-product` attribute holding JSON — name,
id, brand, EAN, price, the full category path — put there for the site's own
analytics. A listing parse is one attribute lookup rather than a walk over
presentational CSS classes, so a restyle does not break it.

That payload is read as a **loose map, not a struct**, and that is not laziness:
its field types are not stable. `productRating` arrives as `5`, as `"4.6"` and
as `"na"` on different tiles of the same listing, and the variation group is
`variationGroupId` on some pages and `variationProductId` on others. A struct
with a declared type per field fails the *whole* payload when any one of them
varies — losing the name, the brand and the barcode because of the rating.

## The cart is five shapes and two ids

Five controllers answer with one model under five names:

| Controller | Names the cart |
| --- | --- |
| `Cart-AddProduct` | `cart` |
| `Cart-UpdateQuantity` | `cartModel` |
| `Cart-SelectStore` | `basketModel` |
| `Cart-RemoveProductLineItem` | `basket` |
| `Cart-MiniCartShow` | no wrapper — the fields are at the top level |

The models are byte-for-byte the same shape; only the key differs. All are
accepted, because missing one is quiet and wrong: the write succeeds and the
basket reads as empty, so `cart remove` reports "removed it" beside an empty
table.

Only the minicart puts `subTotal` beside the items. Every wrapped model keeps it
in `totals`, so reading one name and not the other is a basket that lists its
lines and then claims no total. `quantityTotal` needs the same care from the
other direction: a removal repeats it at the top level for the line it just took
out, where it is always zero, so the model's own count wins.

Each line has **two** ids. `uuid` is the line;
`preOrderUUID` (echoed as `UUID`) is what `Cart-RemoveProductLineItem` accepts —
it refuses the other with a generic "unable to remove". The minicart sends both
names on one item, so they are two fields rather than one aliased field, which
serde rejects as a duplicate.

Setting a quantity to zero is **not** a removal. The site accepts it, reports
success, and leaves the line where it was.

Prices differ too: `price.sales` is per unit, `priceTotal` is the line. Whichever
arrives, [`CartLine`](src/domain.rs) reports both and derives the other, because
one field holding whichever came back is a "Price" that changes meaning between
commands. `priceTotal` itself has two shapes — flat `{"value":…,"formatted":…}`
from the minicart, nested `{"price":{"sales":{…}}}` from every wrapped model —
and both are read, because a dropped total silently becomes the derived one:
right at quantity one, wrong everywhere else.

A write answers with a **partial** basket. `Cart-AddProduct` sends `cartId` and
the lines, and no subtotal, no `totals`, no count. So the answer to a write is
what the site said, not a basket fit to print: a caller showing the result of a
write should re-read the minicart, which is what the site's own page does.

## The wishlist is a page, and its writes disagree with each other

There is no `Wishlist-Show` that answers JSON. The basket has `Cart-MiniCartShow`
to ask; the wishlist has only the rendered `/wishlist` page, read as markup —
and it is an ordinary navigation rather than the `fetch()` shape, which is the
exact opposite of the minicart.

The markup is kinder than a listing, though. A row is a
`<gep-product-card uuid pid addtocarturl quickviewurl>` custom element, so the
ids are the element's own attributes rather than something dug out of prose or a
tracking blob. What the row does *not* carry is a `data-gtm-product` payload, so
there is no brand, EAN or category — and no per-channel stock marker either, only
the cart's phrasing, which is why a saved item keeps a `stock` string where a
tile gets a two-axis [`Availability`](src/domain.rs).

**Each row carries its own add-to-cart token.** This is the one place the
two-step collapses: `addtocarturl` is a pre-signed `/cart/add-product`, so moving
something into the basket needs no product page at all. It expires with the page
it was minted into.

The writes need no token — the `Sec-Fetch-*` headers are the whole guard — and
they still do not agree on a method:

| Controller | How | Takes |
| --- | --- | --- |
| `Wishlist-RemoveProduct` | **GET**, query string | `pid`, `uuid` |
| `Wishlist-UpdateProductQuantity` | **POST**, form body | `uuid`, `pid`, `quantity` |

Two controllers on one page, called by the same script, written the same way in
the page's own `<link rel>` table. Posting to the first is answered with a 500
and a page of apology that says nothing about what it wanted. The cart has the
same split the other way round — everything there is a POST except
`Cart-UpdateQuantity` — so the method is a fact per controller and cannot be
inferred from its neighbours.

Both answer `{"action":…,"success":true}` and nothing else: no list, no count, no
`error` field even to be absent. So a caller that wants to show the result has to
go and read the page again, and `success:false` is the only refusal there is to
see.

The row id is not the product id. `uuid` addresses the list entry, and the
writes take it — a product id alone cannot say which row.

**The list is paged.** The heading counts it (`Wishlist (2)`), a
`Wishlist-MoreList` controller sits behind a "show more" button, and its
parameters were not in any capture — the button renders empty for a list that
fits on one page, and the script that calls it was never captured. So the
heading is how a caller knows the rows it has are not all of them. Failing that,
every page of the site carries `<span data-wishlist-items="R1,R2">` in its
header, which is the whole membership by id whatever the page shows.

## Two shapes of background request

Neither is interchangeable with a page load, and sending the wrong one is
answered with `Cross-Origin Request Blocked`:

| | `fetch` | `XMLHttpRequest` |
| --- | --- | --- |
| Used by | cart, wishlist, product actions, minicart | the search typeahead |
| `Sec-Fetch-Mode` | `same-origin` | `cors` |
| `Accept` | `application/json` | `text/html,application/json;q=0.1` |

And the *writes* are POSTs with a form body — `pid`, `quantity`, `context` —
while the `verify` token stays in the query string. As GETs they answer 500 with
nothing to go on. `Cart-UpdateQuantity` and `Wishlist-RemoveProduct` are the
exceptions and are GETs, which is only ever knowable one controller at a time.

## Listings

One endpoint, `GET /search/updategrid`, serves both search and browse: `q=` for a
keyword, `cgid=` for a category, the same HTML fragment of tiles either way.

Paging is a **window**, not a cumulative refetch — `sz` stays constant while
`start` walks, and random access works, so page *N* costs one request. The end
of the results is a window shorter than the one asked for; the total in the grid
header is a hint, not the stopping condition.

A keyword search can **302 into a category** — `q=lego` has been observed
landing on a brand page — after which paging must use `cgid=`. `Listing::category`
reports that rather than hiding it.

## Sessions

Authorisation is by cookie: `dwsid` (the storefront session), `usid_twl` (the
shopper id) and `cc-at_twl` (a Salesforce SLAS JWT).

**`cc-at_twl` is not a sign-in.** The storefront issues one to every visitor;
an anonymous one carries `upn:Guest::uidn:Guest User` in its `isb` claim.
Testing for the cookie's presence would report a browser that has never signed
in as signed in.

The plainest signal is `cc-nx_twl`, the *registered* refresh token, which only a
successful sign-in sets — the login response expires the guest counterpart
`cc-nx-g_twl` in the same breath. `Session::account()` takes either that or a
non-guest `isb` claim, so neither one going missing costs the answer.

The access token is good for **30 minutes**, which is short enough that a stored
password is the difference between a tool that works and one that asks for a
password every half hour.

Unlike Woolworths, whose session cookie is encrypted and unrenewable, this one
is bought with an ordinary form POST that can simply be re-run — so a lapsed
session can renew itself, given a password.

## Testing

`cargo test` touches no network. The fixtures under [`tests/fixtures`](tests/fixtures)
are real responses from captures of the site, trimmed but not reshaped, so the
tests pin the behaviour of the actual markup. Two are noted as exceptions in the
test module's own doc — one built a case the capture never produced, and the
store list keeps its real envelope around invented records, because the real
ones embedded map URLs signed with The Warehouse's Google API key.
