# projects

One directory per project. Each is self-contained and has its own `Makefile`
exposing the shared targets described in the [root README](../README.md).

Create one with:

```bash
make new TEMPLATE=<c|go|node-ts|python|rust> NAME=<name>
```

Anything here with a `Makefile` is picked up automatically by `make` at the
root and by CI.
