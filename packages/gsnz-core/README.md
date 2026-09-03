# gsnz-core

The grocery domain, with no idea how any supermarket answers a request.

One `Product`, one `Cart`, one `Order`, one `Store`, and a [`Retailer`
trait](src/retailer.rs) a per-vendor adapter implements. This is the vocabulary
a CLI speaking to three chains needs so that the rest of it can be written once.

Nothing here does I/O. The whole dependency list is `serde`, `thiserror` and
the attribute macro for async traits — that is the point, and it is what keeps
the domain reusable by something that is not a CLI.

## What is in it

| Module | What it holds |
| ------ | ------------- |
| [`product`](src/product.rs) | One thing on a shelf: price, unit price, size, stock state, `SaleUnit` |
| [`cart`](src/cart.rs) | The basket, its lines, and `Change` — the one type that travels back to a retailer |
| [`order`](src/order.rs) | Past shops, online and in store, and the filter over them |
| [`store`](src/store.rs) | Where prices come from; nothing is priced until one is chosen |
| [`department`](src/department.rs) | The category tree, as far as either retailer exposes one |
| [`search`](src/search.rs) | `Search`, `SearchBy` and `Sort` — one request shape covering `search`, `specials` and `browse` |
| [`compare`](src/compare.rs) | Lining the same product up across retailers, and recording how |
| [`money`](src/money.rs) | `i64` cents in, dollars out |
| [`retailer`](src/retailer.rs) | `RetailerId`, `Caps`, `Fact`, `AuthStatus` and the `Retailer` trait |
| [`error`](src/error.rs) | What went wrong, as variants a caller can match on |

## The `Retailer` trait

An implementor lives in the app, wraps a vendor API crate
([`fsnz-api`](../fsnz-api), [`wwnz-api`](../wwnz-api)) and converts its types to
these. Adding a fourth supermarket is one more implementor and nothing else.

The optional methods default to a **typed refusal** rather than an empty
result:

```rust
async fn previous_purchases(&self, _max: u32, _exclude_cart: bool) -> Result<Vec<OrderLine>> {
    Err(Error::unsupported(self.id(), "previous purchases"))
}
```

A caller that skips [`Caps`](src/retailer.rs) still fails honestly, instead of
being told it has no past orders when what happened is that this shop cannot
answer the question. `Caps` is the same information ahead of time: the
dispatcher reads it before doing network work, and `doctor` prints it, so a gap
is something you are told about rather than something you discover by hitting
an error.

`facts()` reads nothing over the network on purpose — a health report has to
work when the network is the broken part.

## Errors name a remedy, not a string

The CLIs this replaced classified failures by formatting an `anyhow` chain and
matching substrings against it — `text.contains("401")`, `text.contains("has
expired")`. That works until someone adds a `.context()` line above it.

Here the kind is a variant, decided once at the boundary where the evidence
still exists, and [`Remedy`](src/error.rs) names what to do about it. The
distinctions are the ones that would otherwise waste someone's time:

- `SessionExpired { renewable }` versus `LoginRefused` — telling someone to run
  `auth refresh` when they mistyped a password sends them to a command that
  cannot help.
- `CartUnbound` versus `NoStore` — one is a server-side fact about the account's
  cart, the other a local setting. `store set` fixes only the second.

The app turns a `Remedy` into the command a person types, because only the app
knows what it is called.

## Comparison records how it matched

New World and PAK'nSAVE share one Foodstuffs catalogue, so a SKU joins them
exactly. Woolworths is a different company with its own product codes, and
nothing joins the two catalogues exactly — so `pair` falls back to brand, name
and canonicalised size (`2L`, `2 litre` and `2000ml` fold together), and every
`Row` records which tier produced it.

That marker is not decoration. A comparison that silently equates two different
two-litre milks is a wrong-price bug, which is the worst kind this software can
have, so the caller can refuse the second tier outright with
`pair(&sides, false)`.

Money is `i64` cents everywhere inside and dollars only on the way out. Nothing
here is a float: a price that has been through an `f64` is a price that can
print as `$4.289999999`.

## Development

```bash
dispat run check --since all -p gsnz-core
```

Unit tests beside the code, no network and no fixtures — the crate cannot make
a request.

Used by [`gsnz-ui`](../gsnz-ui) and both apps. Not published to crates.io;
consumers declare a path dependency, as [`packages/README.md`](../README.md)
describes.
