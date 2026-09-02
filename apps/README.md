# apps

One directory per app — the things that actually ship. Each is self-contained
and has its own `dispat.yaml` declaring the scripts described in the
[root README](../README.md).

| App | Binary | What it is |
| --- | ------ | ---------- |
| [`foodstuffs-nz-cli`](foodstuffs-nz-cli) | `fsnz` | New World and PAK'nSAVE: search, specials, cart, orders, and one query priced at both banners |
| [`woolworths-nz-cli`](woolworths-nz-cli) | `wwnz` | Woolworths NZ: the same, against their GraphQL API |

Create one with:

```bash
scripts/new-project.sh <c|go|node-ts|python|rust> <name>
```

Anything here with a `dispat.yaml` is picked up automatically by dispat and by
CI. The table above is for readers — neither of them reads it, so a new app
only has to be a directory.

Code shared between two apps goes in [`packages/`](../packages) instead.
