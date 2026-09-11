# gsnz-core

The grocery domain, with no idea how any supermarket answers a request.

One `Product`, one `Cart`, one `Order`, one `Store`, and a [`Retailer`
trait](src/retailer.rs) a per-vendor adapter implements. Nothing here does I/O:
the whole dependency list is `serde`, `thiserror` and the attribute macro for
async traits.

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

So a caller that skips [`Caps`](src/retailer.rs) still fails honestly rather
than being told it has no past orders. `Caps` is the same information ahead of
time: the dispatcher reads it before doing network work, and `doctor` prints
it.

`facts()` reads nothing over the network — a health report has to work when the
network is the broken part.

## Errors name a remedy

The kind is a variant, decided at the boundary where the evidence still exists,
and [`Remedy`](src/error.rs) names what to do about it. Two distinctions worth
keeping apart:

- `SessionExpired { renewable }` versus `LoginRefused` — `auth refresh` cannot
  help someone who mistyped a password.
- `CartUnbound` versus `NoStore` — one is a server-side fact about the
  account's cart, the other a local setting. `store set` fixes only the second.

The app turns a `Remedy` into the command a person types, because only the app
knows what it is called.

## Comparison records how it matched

New World and PAK'nSAVE share one Foodstuffs catalogue, so a SKU joins them
exactly. Nothing joins the Woolworths catalogue exactly, so `pair` falls back to
brand, name and canonicalised size (`2L`, `2 litre` and `2000ml` fold
together), and every `Row` records which tier produced it. A caller can refuse
the second tier outright with `pair(&sides, false)`.

Money is `i64` cents everywhere inside and dollars only on the way out. Nothing
here is a float.

## Development

```bash
dispat run check --since all -p gsnz-core
```

Unit tests beside the code, no network and no fixtures — the crate cannot make
a request.

Used by [`gsnz-ui`](../gsnz-ui), [`gsnz`](../../apps/grocery-nz-cli),
[`fsnz`](../../apps/foodstuffs-nz-cli) and
[`wwnz`](../../apps/woolworths-nz-cli). Not published to crates.io; consumers
declare a path dependency.
