# Changelog

## gsnz-core/v0.1.0 (2026-09-03)

### Features

- expose each crate's own version
  One subject for seven scopes because it is one line in each, and the reason
  is the same everywhere: `env!` expands where it is written, so a consumer
  asking for `CARGO_PKG_VERSION` gets its own version back. A crate that wants
  to report what it was built against has to be told by the crate itself.

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

- retailer-neutral grocery domain
  The vocabulary a combined CLI needs, so the per-vendor crates do not each
  invent a Product. Quantity unifies Foodstuffs' (u32, SaleType) with
  Woolworths' f64; Cart keeps fees as a list so a new one is a new row
  rather than a lost number; Error replaces classifying failures by
  substring-matching a formatted anyhow chain.

  compare::pair joins the Foodstuffs banners on SKU exactly, then falls
  back to normalised brand/name/size for Woolworths, which is a different
  catalogue. Every row records which tier matched it: a comparison that
  silently equates two different 2L milks is a wrong-price bug.
