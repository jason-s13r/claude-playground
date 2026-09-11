# gsnz-ui

Showing groceries to a person.

Every type here is a [`cli_kit::View`](../cli-kit/src/out.rs) over a
[`gsnz-core`](../gsnz-core) type, so `--json` falls out of the same struct the
text renderer reads.

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

A listing ends with a count and a suggestion — `3 stores. Select one: gsnz
store set <id>`. The command in that suggestion is passed in, never composed
here: this crate does not know what the binary it is linked into is called, and
three binaries link it.

## Products are grouped, not tabulated

A product has a price, a unit price, a size, a stock state and sometimes a
multi-buy, and few products have all of it — so columns spend most of the
terminal width on empty cells. Grouping by title also puts the size variants of
one product together.

Carts, orders, stores and comparisons are tables; their rows are uniform. The
department tree is indented, because the shape is the information.

## The `~` marker in a comparison

A row matched by description rather than by shared product code is marked, and
the marker is explained under the table. The domain decides which rows those
are — see
[`gsnz-core`](../gsnz-core/README.md#comparison-records-how-it-matched) — this
crate keeps them visibly distinct.

## Development

```bash
dispat run check --since all -p gsnz-ui
```

Every renderer is unit-tested against `Out::buffer`, so the assertions are on
the actual text and the actual JSON.

Used by [`gsnz`](../../apps/grocery-nz-cli),
[`fsnz`](../../apps/foodstuffs-nz-cli) and
[`wwnz`](../../apps/woolworths-nz-cli). Not published to crates.io; consumers
declare a path dependency.
