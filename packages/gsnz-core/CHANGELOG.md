# Changelog

## gsnz-core/v0.2.0 (2026-09-03)

### Features

- bind the account's cart to a store
  `store set` chose which store searches were scoped to and left the cart alone,
  so a freshly signed-in account could search but every `cart add` was refused
  with "Store is not defined" and nothing here could fix it.

  The account's cart carries its own store, and one bare POST sets it:

      POST /v1/edge/cart/store/{storeId}    no body, empty 200

  Found by recording a store change on the site; the earlier capture happened
  not to contain one, which is why an endpoint was assumed not to exist. The
  store cookies on the storefront are the browser's own copy, not the source of
  truth.

  `store set` now writes the local preference and binds the cart, so it means
  the same thing on all three shops, and Foodstuffs claims server_side_store
  like Woolworths already did. Signed out there is no cart to bind and browsing
  still works, so that case is passed over rather than raised.

  Verified against a live account: the cart reported no store, `store set` bound
  it to PAK'nSAVE Whangarei, and an add that had been refused went through.

- make doctor a report that checks things
  Reshaped to match `fsnz doctor`: a header block of what the tool decided, then
  one section per shop of indented label/value lines, then a verdict. The old
  table had a column the status could not name and painted cells that
  box-drawing measured wider than they looked.

  It now also *checks*. One store call per shop says whether the chain works,
  which is a different claim from "configured" and the more useful one, and the
  store list it returns names the selected store for free. Adapters describe
  their own hostnames and token state through Retailer::facts, because what is
  worth reporting differs: Foodstuffs has two hostnames and a mintable token,
  Woolworths has a storefront and an Auth0 tenant.

  Three things found while doing it:

  - `retailer = "nw"` in the config file was rejected while `-b nw` worked, and
    RetailerId serialised as "new-world" while id() said "newworld". It now
    deserialises through FromStr and serialises through id(), so the file takes
    what the flag takes and there is one machine spelling. This changes `--json`:
    "new-world" becomes "newworld". gsnz-ui's stability test caught it, which is
    what that test is for; the tool is a day old and two spellings of a machine
    name would outlive that.
  - doctor exited 0 however badly it went. AppError::Reported carries an exit
    code without a message, since the report already said everything.
  - the suite started making real calls to three supermarkets. Every host is
    pointed at a closed port in the sandbox, which also took the run from 10.5s
    back to 1.1s.

### Fixes

- complete the variant key, and stop misreporting an unbound cart
  `gsnz -b ww cart add 282848` sent the stock code as the variant key. The site
  does not reject that as unknown -- it answers "these items are no longer
  available at this store", which reads as the product being gone, so every add
  looked like a stock problem. Search prints the stock code and that is what
  people type, so it is completed to `282848-EA` before it is sent, or `-KGM`
  when kilograms were asked for.

  `gsnz -b pns cart add` reported "no PAK'nSAVE store selected: run `store set`"
  when a store *was* selected. Two different problems shared one error: the
  local setting this tool owns, and the store the account's cart is bound to
  server-side. `store set` writes a config file and does not touch the cart, so
  the advice was a dead end. Error::CartUnbound says what it is and points at
  the website, which is the only place that binding can be changed.

  Also: `store set` printed the store *listing* view, whose footer says "1
  store. Select one: gsnz store set ...", so a successful set read as though
  nothing had happened. And an error whose message already contained its
  source's words printed them twice, which read as two problems.

- a refused sign-in is not an expired session
  A wrong Woolworths password reported "the Woolworths session has expired" and
  suggested `gsnz auth refresh` -- a command that cannot help, for a session
  that never existed. The upstream error is LoginRefused, whose Fault is
  AuthFault::Rejected, which the adapter mapped to SessionExpired along with
  every other rejection.

  The same upstream error means different things depending on what was being
  attempted, so each adapter now reads failures from its login through a
  separate mapping. Error::LoginRefused carries the reason, exits 3 like the
  other auth failures, and offers no hint, because there is nothing useful to
  suggest beyond the password.

  Two more from the same transcript:

  - `auth login` and `auth refresh` abandoned every remaining account at the
    first failure, and threw away the statuses of the ones that had already
    worked. They now collect failures, print what happened to every account,
    and return the first so the exit code is still right.
  - `auth refresh` treated "never signed in" as a failure. It is a note now:
    refreshing everything is maintenance, and one shop being signed out is not
    a reason for it to fail.

  Both login chains are now verified against the real services.


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
