# gsnz-ui

Showing groceries to a person.

Every type here is a [`cli_kit::View`](../cli-kit/src/out.rs) over a
[`gsnz-core`](../gsnz-core) type, which is the whole shape of the crate: the
domain knows nothing about rendering, the rendering knows nothing about HTTP,
and `--json` falls out of the same struct the text renderer reads rather than
being written twice.

## The views

| Type | What it renders |
| ---- | --------------- |
| [`ProductList`](src/products.rs) | A search, browse or specials result |
| [`CartView`](src/cart.rs) | The cart, its lines and the money underneath it |
| [`OrderList`](src/orders.rs) / [`OrderDetail`](src/orders.rs) | Past shops, as a list and one in full |
| [`StoreList`](src/stores.rs) | Stores, for picking one |
| [`DepartmentTree`](src/departments.rs) | The category tree, indented |
| [`CompareTable`](src/compare.rs) | Retailers side by side |

```rust
use cli_kit::{emit, Format, Out};
use gsnz_ui::{ProductList, StoreList};

let mut out = Out::stdout(Format::Text, no_color);
emit(&mut out, &ProductList::new(&products, retailer).at(store).of(total))?;
emit(&mut out, &StoreList::new(&stores).next("gsnz store set <id>"))?;
```

`next` is the part worth knowing. A listing ends with a count and a suggestion —
`3 stores. Select one: gsnz store set <id>` — and the command in that suggestion
is passed in, never composed here. This crate does not know what the binary it
is linked into is called, and two binaries do link it.

## Why a product listing is not a table

A product has a price, a unit price, a size, a stock state and sometimes a
multi-buy. Squeezing that into columns spends most of the terminal width on
empty cells, because few products have all of it. Grouping by title instead puts
the size variants of one product together rather than scattering them through
the results.

Carts, orders, stores and comparisons *are* tables — their rows really are
uniform.

The department tree is indented rather than tabulated for the same kind of
reason: the shape is the information, and a table would flatten it away.

## The `~` marker in a comparison

A row matched by description rather than by shared product code is marked, and
the marker is explained under the table. This view must never present a guess as
a fact: a comparison that silently equates two different two-litre milks is a
wrong-price bug. The domain decides which rows those are — see
[`gsnz-core`](../gsnz-core/README.md#comparison-records-how-it-matched) — this crate only
has to keep them visibly distinct.

## Development

```bash
dispat run check --since all -p gsnz-ui
```

Every renderer is unit-tested against `Out::buffer`, so the assertions are on
the actual text and the actual JSON rather than on a process's stdout.

Used by both apps. Not published to crates.io; consumers declare a path
dependency, as [`packages/README.md`](../README.md) describes.
