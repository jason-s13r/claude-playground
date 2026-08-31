# projects

One directory per project. Each is self-contained and has its own
`dispat.json` declaring the scripts described in the
[root README](../README.md).

Create one with:

```bash
scripts/new-project.sh <c|go|node-ts|python|rust> <name>
```

Anything here with a `dispat.json` is picked up automatically by dispat and by
CI. There is no list to update.
