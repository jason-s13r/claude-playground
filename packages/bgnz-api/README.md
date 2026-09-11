# bgnz-api

The Briscoe Group NZ storefronts — **Briscoes and Rebel Sport** — as one client:
catalogue, search, stores, click-and-collect stock, cart, wishlist, orders and
loyalty.

> Reverse-engineered from the sites' own traffic. There is no public API and no
> documentation; these endpoints can change without notice.

## One backend, two fascias

Both sites are a single Magento 2 deployment with a PWA Studio frontend. Which
catalogue answers is decided by a `store` request header, and the storefront's
own `vary` confirms it:

```
vary: Accept-Encoding,Store,Content-Currency,Authorization,X-Magento-Cache-Id
```

Either public host answers for either fascia. That is why this is one crate with
a `Banner` parameter rather than two crates.

It is **not** the Foodstuffs arrangement, despite the resemblance. New World and
PAK'nSAVE share one Club Plus login across two banners; these two share a backend
but have separate Gigya sites with separate API keys, so a Briscoes sign-in is
not a Rebel Sport one and credentials are filed apart. Nor is there anything to
compare between them — the SKU namespaces are disjoint (`1xxxxxx` and `8xxxxxx`)
and so are the catalogues.

| | Briscoes | Rebel Sport |
| --- | --- | --- |
| `store` header | `briscoes` | `rebelsport` |
| Klevu key | `klevu-1731900001…` | `klevu-1731900291…` |
| Gigya API key | `4_R5DCbpg6mrf…` | `4_27oU1279wfz…` |
| Categories | 961 | 1,004 |

Every one of those values except the header is read from `storeConfig` at run
time, not compiled in.

## Three surfaces

| Surface | Needs a token | Needs the `store` header |
| --- | --- | --- |
| Klevu — search, browse | no | no, the API key carries the fascia |
| GraphQL — catalogue, stores | no | **yes** |
| GraphQL — cart, wishlist, orders, loyalty | **yes** | **yes** |
| REST — click-and-collect stock | no | **yes** (in the body) |

The `store` header is the one that bites. Magento does not reject a request
without it — it serves the default store, Briscoes — so a forgotten header on a
Rebel Sport command answers with homeware and no error at all.

## Signing in

The whole difficulty sits in one call. Gigya's `accounts.login` refuses any
request without a reCAPTCHA token, and refuses it *before* checking the
password:

```
errorCode 400006, "Invalid CaptchaType / Invalid CaptchaToken"
```

There is no headless username-and-password path. But everything after that is
unguarded — `accounts.getAccountInfo` carries no risk assessment, and no GraphQL
operation on either site sends the `X-ReCaptcha` header its bundle can produce.
So:

```
once, with a browser   password + captchaToken → accounts.login → login_token
every time after       login_token → accounts.getAccountInfo → UIDSignature
                       UIDSignature → loginGigya → a 120-minute Magento JWT
```

`auth::login` therefore *takes* a captcha token rather than minting one: a
browser is the app's business. The stored credential is the Gigya `login_token`,
never a password — and because that token is also the value of the site's own
`glt_<apiKey>` cookie, pasting one is a reasonable alternative to driving a
browser at all.

## What is in it

| Module | What it does |
| ------ | ------------ |
| [`banner`](src/banner.rs) | The two fascias, and the header that switches them |
| [`client`](src/client.rs) | One client over all three surfaces |
| [`search`](src/search.rs) | Klevu: keyword search, category browse, facets, paging |
| [`stock`](src/stock.rs) | Click-and-collect, and its unusually strict validator |
| [`auth`](src/auth.rs) | Gigya → Magento, and where the captcha sits |
| [`session`](src/session.rs) | The two credentials, filed per fascia |
| [`gql`](src/gql.rs) | The GraphQL documents, named as the site names them |
| [`wire`](src/wire.rs) | The loose types, because nothing arrives well-typed |

## Two things the traffic capture could not show

Both were found by pointing the crate at the live sites, and both are silent
failures rather than errors:

**`selectedStoreId` is the store's `store_id`, not its `fulfilment_number`** —
the opposite of `setStoreLocator`, which takes the fulfilment number. Sending
the wrong one answers `NOT_FOUND_STORE` with `success: true`.

**A configurable product has no barcode; its variants do.** The stock service
identifies a product by barcode, so a Rebel Sport shoe has to be asked about by
variant. `ProductDetail::stockable` is what resolves that.

A third worth knowing: the stock service answers **per basket, not per line**.
Send two line items and the worse of them decides the single verdict, so
`Client::stock` and `Client::basket_stock` are separate calls rather than one
that gets sliced up afterwards.

## Everything arrives loose

Not hypothetically. In one answer from one product query, `isdropship` is the
integer `0` where a boolean belongs, `sapcategory` is `"141"` on one product and
`141` on the next, and `saleavailability` is the integer `20127` while the
`_label` beside it is `null` — and the stock service demands a *string* there.
Klevu returns every price quoted. A store's opening hours arrive as a JSON
document nested inside a JSON string.

So every field is `Option` or defaults: a field either vendor renames should cost
a column, not a command.
