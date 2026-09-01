# projects

One directory per project. Each is self-contained and has its own
`dispat.json` declaring the scripts described in the
[root README](../README.md).

| Project | Binary | What it is |
| ------- | ------ | ---------- |
| [`foodstuffs-nz-cli`](foodstuffs-nz-cli) | `fsnz` | New World and PAK'nSAVE: search, specials, cart, orders, and one query priced at both banners |
| [`woolworths-nz-cli`](woolworths-nz-cli) | `wwnz` | Woolworths NZ: the same, against their GraphQL API |

Create one with:

```bash
scripts/new-project.sh <c|go|node-ts|python|rust> <name>
```

Anything here with a `dispat.json` is picked up automatically by dispat and by
CI. The table above is for readers — neither of them reads it, so a new
project only has to be a directory.
