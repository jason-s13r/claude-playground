# the-warehouse-nz-cli

Search and shop [The Warehouse](https://www.thewarehouse.co.nz) New Zealand from
the terminal. Ships the binary `twlnz`.

> **Not affiliated with The Warehouse.** There is no public API. This calls the
> same undocumented endpoints their website calls from the browser, and can
> break whenever they change something. Use at your own risk.

## How it is built

`twlnz` is a thin front end. The part worth reading is in `packages/`:

| Crate | What it holds |
| --- | --- |
| [`twlnz-api`](../../packages/twlnz-api) | the storefront: listings, products, cart, wishlist, stores — in its own vendor-shaped types |
| [`cli-kit`](../../packages/cli-kit) | tables, `--json`, prompts, `doctor`, completions |
| [`net-kit`](../../packages/net-kit) | the browser-fingerprinted HTTP client, cookies and the credential store |
| [`build-kit`](../../packages/build-kit) | the build stamp and `twlnz update` |

What is left in `src/` is the part that is genuinely this program: reading the
environment once, resolving flags against config, rendering, and turning a
failure into an exit code.

### Standalone from the grocery tools

Unlike [`wwnz`](../woolworths-nz-cli) and [`fsnz`](../foodstuffs-nz-cli), this
does **not** build on `gsnz-core` and `gsnz-ui`, and there is no Warehouse
adapter in [`gsnz`](../grocery-nz-cli). The Warehouse is general merchandise
that happens to sell food, so the grocery domain is the wrong vocabulary:
`gsnz_core::Product` has a `SaleUnit` and nowhere to put a colour or size axis,
and this catalogue is full of variation masters.

`cli-kit` is domain-free by construction — *"a table is a table whether it holds
groceries or anything else"* — so the shared half comes along and the groceries
do not. The cost is the handful of `View` impls in [`src/views/`](src/views),
which is cheap and buys types that fit the retailer.

## Install

```bash
cargo build                        # from this directory
cargo install --path .             # or install the `twlnz` binary
```

Or take a published build from
[releases](https://github.com/jason-s13r/claude-playground/releases), tagged
`the-warehouse-nz-cli/vX.Y.Z`. Once you have a binary it can replace itself:

```bash
twlnz update --check     # is there a newer one, and what changed in it?
twlnz update             # download it and swap it in
```

## Use

```bash
twlnz search "lego" --limit 10
twlnz search "tee" --brand "H&H" --color "Blue Dark" --sort price-low-to-high
twlnz departments                       # the category tree, with the ids `browse` takes
twlnz browse toysbaby --limit 20
twlnz specials

twlnz product RM110166766-10M           # price, variations, per-channel stock
twlnz product RM110166766-10M --select size=XL
twlnz stock R3035996 --region canterbury

twlnz stores whangarei                  # searched nationwide
twlnz stores --region canterbury         # or one region, listed in full
twlnz store set 116                     # found anywhere, no region needed

twlnz island set south                  # north/south: what a listing contains
twlnz region set canterbury             # NZ-CAN: which shops get asked

twlnz auth login
twlnz cart add R3059518 2
twlnz cart list

twlnz wishlist                          # what is saved
twlnz wishlist add R3059518
twlnz wishlist set R3059518 2           # how many are wanted; 0 stops saving it
twlnz wishlist move-to-cart R3059518
```

Every command takes `--json`, and it is the same data the table is built from
rather than a second rendering.

### `island` and `region` are different things

The site calls both of them "region". This does not, because they answer
different questions and conflating them means silently moving one while setting
the other. Both are `show` / `list` / `set` / `clear`.

| | `twlnz island` | `twlnz region` |
| --- | --- | --- |
| Values | `north`, `south` | the sixteen `NZ-` codes |
| Decides | **what a listing contains** | **which shops get asked** |
| Used by | `search`, `browse`, `specials` | `stores`, `stock` |
| Override for one run | `--island` | `--region` |

The island is not cosmetic: The Warehouse ranges differently north and south, so
a product genuinely absent from one island's results is on the shelf on the
other.

### Finding a store

`twlnz stores <name>` searches **nationwide**. The finder is per region and
there is no call that lists them all, so the whole directory — about 84 shops —
is fetched once, all sixteen regions at a time, and cached for a week. After the
first run it is instant and works offline; `--refresh` re-fetches it, and
`twlnz doctor` says how old it is.

The sixteen lookups go out four at a time rather than all at once — the requests
do not depend on each other, but a sixteen-wide burst is the shape that gets a
client throttled regardless of how little it asks for overall. Four is roughly
what a browser opens to one host, and this runs about once a week.

With no name to search for it lists one region instead, because two hundred rows
is not a listing anyone reads.

`store set` uses the same directory, so an id copied out of any listing works
without also saying which region it came from.

### A cart write costs one extra read

The site answers a write with a partial basket — the lines, and no subtotal or
count — so `cart add` and `cart remove` re-read the minicart before printing.
That is one small GET, it is what the site's own page does after a write, and it
is the difference between a table that matches `cart list` and one that quietly
means something else.

### `wishlist` shows the list without being asked

`twlnz wishlist` prints what is saved. There is no `wishlist list` to type,
because reading is what a wishlist is mostly for — `list` is accepted anyway, so
the habit is never punished.

The rest are `add`, `remove`, `set` and `move-to-cart`, all by product id.
Internally the site addresses a saved row by a `uuid` that a person never sees,
so every one of these reads the list first to turn the id into the row.

`move-to-cart` is **two changes**: the product goes into the cart, then off the
list, because that is what the site's own button is. They are done in that order
on purpose — a failure in between leaves it in the cart and still saved, rather
than in neither. It also defaults to the quantity saved against the row, since
that is the number someone put there.

A saved row carries its own add-to-cart token, so `move-to-cart` does not fetch
the product page the way `cart add` has to.

Saving is not buying: the quantity is a note to self, nothing is reserved, and
the site quotes no total for it — so the table has one price column where `cart`
has two.

### The store is a local preference

`store set` records the store here and pulls its region along, so `stock` and
`stores` then default to where it actually is rather than to Auckland. It does
not bind the store server-side: `Cart-SelectStore` needs a basket to bind a
collection point to and answers an empty one with a 500. That belongs to
checking out, which this tool does not do.

### Stock has two axes

An item can be orderable online, orderable only by walking into a shop, both, or
neither. `twlnz search` prints `in store` for the second, never `sold out` —
collapsing that to a boolean would print exactly the wrong thing for a shelf
full of stock.

```
$ twlnz product RM110166766-10M --select size=XL
H&H Men's Regular Fit Crew Neck Tee
$6.99  in store
Online out of stock
```

### Variations

A listing links to a variation group; the cart takes a variant. `twlnz product`
shows every axis with three states, because "not made in this combination" and
"sold out" send you in different directions:

```
Color (Blue Dark)
    Beige
  * Blue Dark          <- chosen
  x Blue Mid sold out
    Brown Dark
```

## Configuration

`twlnz config list` shows every setting, what it is, and what it does.
Precedence is **flag, then environment, then config file, then the default** —
applied in one place so no command has to remember it.

| Variable | What it moves |
| --- | --- |
| `TWLNZ_CONFIG_DIR` / `TWLNZ_STATE_DIR` | where config and state live |
| `TWLNZ_SECRET_BACKEND` | `keyring` or `file` |
| `TWLNZ_ORIGIN` | the storefront, for pointing at a mock server |
| `TWLNZ_DEBUG` | narrate requests on stderr — cookie names only, no query strings |
| `NO_COLOR` | honoured whatever the config says |

## Exit codes

So a script can tell failures apart without reading the message.

| Code | Means |
| --- | --- |
| 0 | fine |
| 1 | something went wrong |
| 2 | you typed it wrong |
| 3 | sign in, or sign in again |
| 5 | no such store or region |
| 7 | the site is rate-limiting; back off |
