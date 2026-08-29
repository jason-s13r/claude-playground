# Workflows

`ci.yml` discovers every project under `projects/` and runs `make check` for
each one in its own job. Adding a project to `projects/` is all it takes to get
it into CI -- there is no list to update.
