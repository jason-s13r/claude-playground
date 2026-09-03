# Changelog

## gsnz-ui/v0.2.0 (2026-09-03)

### Features

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

### Dependencies

- gsnz-core: 0.1.0 -> 0.2.0
- cli-kit: 0.1.0 -> 0.2.0


## gsnz-ui/v0.1.0 (2026-09-03)

### Features

- expose each crate's own version
  One subject for seven scopes because it is one line in each, and the reason
  is the same everywhere: `env!` expands where it is written, so a consumer
  asking for `CARGO_PKG_VERSION` gets its own version back. A crate that wants
  to report what it was built against has to be told by the crate itself.

- grocery renderers over the shared domain
  Views for products, cart, stores, orders, departments and comparison.
  Each is a cli_kit::View over a gsnz_core type, so --json falls out of the
  same struct the text renderer reads instead of being written twice.

  The comparison table marks a column matched by description rather than
  product code with ~, and explains the marker beneath the table. Only a
  Foodstuffs pairing is exact; a Woolworths column attached by name and size
  is a guess and must not read as a fact.

  Product listings stay a grouped indented list rather than a table: a
  product has a price, a unit price, a size, a stock state and sometimes a
  multi-buy, and columns waste most of the width on empty cells.

### Dependencies

- gsnz-core: 0.0.0 -> 0.1.0
- cli-kit: 0.0.0 -> 0.1.0
