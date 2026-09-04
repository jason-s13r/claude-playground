# apps

One directory per app — the things that actually ship. Each is self-contained
and has its own `dispat.yaml` declaring the scripts described in the
[root README](../README.md).

| App | Binary | What it is |
| --- | ------ | ---------- |
| [`grocery-nz-cli`](grocery-nz-cli) | `gsnz` | All three supermarkets: search, specials, browse, cart, orders, and one query priced at New World, PAK'nSAVE and Woolworths at once |
| [`foodstuffs-nz-cli`](foodstuffs-nz-cli) | `fsnz` | The Foodstuffs banners on their own: New World and PAK'nSAVE, `compare` across the two |
| [`woolworths-nz-cli`](woolworths-nz-cli) | `wwnz` | Woolworths NZ on its own, against their GraphQL API |
| [`the-warehouse-nz-cli`](the-warehouse-nz-cli) | `twlnz` | The Warehouse: search, browse, per-store stock, variations, cart and wishlist |

They overlap on purpose. `gsnz` is the whole of it: the libraries in
[`packages/`](../packages) with a `Retailer` adapter per chain. `fsnz` and
`wwnz` are the single-chain slices of that same architecture, each dropping the
API crate it does not speak — `wwnz-api` and `fsnz-api` respectively — and with
it the flags that only make sense with a second shop. Each keeps its own
config, state and credentials, so having more than one installed is not a
conflict.

`twlnz` is not part of that family. It is a different retailer in a different
trade, sharing only the halves that have no domain in them, and there is no
Warehouse adapter in `gsnz`. Its own README says why.

Create one with:

```bash
scripts/new-project.sh <c|go|node-ts|python|rust> <name>
```

Anything here with a `dispat.yaml` is picked up automatically by dispat and by
CI. The table above is for readers — neither of them reads it, so a new app
only has to be a directory.

Code shared between two apps goes in [`packages/`](../packages) instead.
