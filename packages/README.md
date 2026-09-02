# packages

Libraries the apps share. One directory per package, self-contained the same
way an app is, with its own `dispat.yaml` declaring its scripts.

Nothing lives here yet. The space exists so that the first piece of code two
apps both need has somewhere to go that is not a copy-paste.

```bash
scripts/new-project.sh --space packages <c|go|node-ts|python|rust> <name>
```

The templates are app-shaped — a hello-world with a `main` and a test — so
scaffolding a library means deleting the entry point afterwards. A library
template gets added when a second library wants one.

## When something belongs here

When a *second* app needs it. One app needing it means it belongs in that app;
extracting a library ahead of its second caller guesses at the interface, and
the guess is usually wrong. A package with one consumer is an app's own module
that has been moved further away.

## Depending on one

Apps do not find packages automatically — a dependency is declared twice:

1. In the app's own manifest, however that language does it. For Rust that is a
   path dependency:

   ```toml
   my-lib = { path = "../../packages/my-lib", version = "0.1.0" }
   ```

2. In the root [`dispat.yaml`](../dispat.yaml), so dispat orders the builds and
   propagates version bumps:

   ```yaml
   dependencies:
     my-app: [my-lib]
   ```

Then a commit that releases the library can carry the bump onward:

```
feat(my-lib)^: add a streaming parser
```

`^` releases the direct consumers too, `^^` the transitive ones. Left off, only
the library releases. dispat rewrites the version in the consumer's manifest to
whatever it just published, so the path stays a path and the number stays
honest.

This is the only sanctioned way one directory may reference another. It does
not make them a workspace: each still owns its manifest, its lockfile and its
own `dispat.yaml`.
