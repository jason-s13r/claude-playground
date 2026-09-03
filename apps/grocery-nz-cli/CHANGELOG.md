# Changelog

## grocery-nz-cli/v0.1.0 (2026-09-03)

### Features

- list the library versions in --version
  The seven packages release on their own tags, so "gsnz 0.1.0" alone does not
  say which fsnz-api is compiled in -- and that is the part that breaks when a
  supermarket changes its API. One per line, aligned under the label column:
  seven of them comma-run together is a wall, and the version is the part being
  looked for.

  `-V` stays one line, which is what a script greps and what a bug report
  pastes; the libraries, the binary path and the install record go on
  `--version`.

  Overrides::get memoises the environment read, because clap builds the version
  string before App exists and the install record lives under a state directory
  the environment can move.

- Retailer::login, auth, doctor and update
  One subject for both because it is one change: the trait method and its only
  callers. A required method cannot land in gsnz-core ahead of the adapters that
  implement it without leaving a commit that does not compile, and a default
  returning Unsupported would be a lie -- both retailers do support signing in.

  login takes a code callback rather than a second command: the token a Club
  Plus challenge hands back only means anything inside the exchange that
  produced it, so splitting the flow across two runs would mean persisting a
  half-finished login.

  doctor prints the capability matrix before anything runs into it. update takes
  the file to replace from build-kit rather than reading current_exe deep
  inside, and stays silent on stderr under --json.

  Also: release-build.sh, the release flow and platforms, and a README.

  Not covered by any test: `auth login` against real Club Plus and real Auth0.
  Both chains are only exercised against mocks, and that is a manual step.

- cart, orders and compare
  compare fans out with tokio tasks and reports a shop that could not answer on
  stderr rather than failing: a lapsed Woolworths session must not hide the two
  Foodstuffs prices. Rows matched by description rather than product code are
  marked, and --strict drops them.

  `-b` now takes a list so compare can span a subset; every other command
  insists on exactly one and says so.

  cart add reads the current line before writing, because both APIs take an
  absolute quantity per line and neither has an increment -- and a line priced
  by the kilogram counts the addition in kilograms, whatever the flag says.

- the app skeleton, both adapters and the first commands
  Consumes all seven packages for the first time. Two Retailer implementations
  -- Foodstuffs parameterised by banner and instantiated twice, Woolworths once
  -- with their conversions, a lazy Registry, and search/specials/browse/
  departments/stores/store/completions on top.

  src/env.rs is the only std::env::var in the tree; the libraries take values,
  and a clippy.toml in each enforces it.

  Woolworths refuses a per-command --store rather than ignoring it: prices are
  quoted against the store the cart is bound to server-side, so honouring the
  flag is impossible and ignoring it would be a wrong-price bug.

### Fixes

- declare the build-dependency as an inline entry
  dispat rewrites the version of an inline dependency entry on release. It
  rewrote all seven in [dependencies] and did not see the eighth, an eighth
  declaration of build-kit written as a [build-dependencies.build-kit]
  sub-table -- so that one stayed at ^0.0.0 while build-kit became 0.1.0, and
  syncLock failed with "failed to select a version".

  The ranges are at ^0.1.0 here too: the seven libraries released, this did
  not, so its manifest was the one thing left pointing at versions that no
  longer exist.

- drop the doubled colon in the auth prompts
  `cli_kit::prompt` appends ": " itself, so "Email: " rendered as "Email: : ".
  Its no-terminal error reads "{label} is required", which is what the
  regression test asserts on -- and which only reads correctly with a bare
  label either.
