# Changelog

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
