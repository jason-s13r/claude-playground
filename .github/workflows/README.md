# Workflows

`ci.yml` runs `dispat run check --since all` on every push and pull request.
dispat discovers the projects under `projects/` itself, so adding one is all it
takes to get it into CI -- there is no list to update, and a project that
defines no `check` script is skipped rather than failed.

`release.yml` runs on every push to `main`. dispat reads the conventional
commits since each project's last tag, decides which projects changed and what
their next versions are, and releases only those; a push with nothing
releasable in it does nothing. Projects shipping binaries for more than one
platform are built once per platform, because there is no cross-compiling, and
the artifacts are gathered into one release with a `SHA256SUMS` covering all of
them.
