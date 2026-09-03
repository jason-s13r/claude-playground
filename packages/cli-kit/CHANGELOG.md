# Changelog

## cli-kit/v0.2.0 (2026-09-03)

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

### Fixes

- report health as a list, and only show real gaps
  The report was a table whose first column had no header, because the status
  has nothing to call itself. It was also painted, and box-drawing measured the
  escape codes as width, which pushed every border out of line. An aligned list
  has neither problem, and puts a hint under the detail it belongs to.

  The capability matrix is gone from an ordinary run. It exists to surface gaps,
  and there are none left -- so it printed three columns of "yes", which is a
  table that says nothing. It comes back on its own the day a shop cannot do
  something, which is the only day it is worth reading.


## cli-kit/v0.1.0 (2026-09-03)

### Features

- expose each crate's own version
  One subject for seven scopes because it is one line in each, and the reason
  is the same everywhere: `env!` expands where it is written, so a consumer
  asking for `CARGO_PKG_VERSION` gets its own version back. A crate that wants
  to report what it was built against has to be told by the crate itself.

- re-export comfy-table and serde_json
  A consumer building rows for `table()` or overriding `View::json` names
  those crates' types in code this one returns. Two majors of either in one
  dependency tree would surface as a baffling trait mismatch rather than as a
  version error, and every crate here has its own lockfile.

- output routing, tables, prompts, health reports
  Both existing CLIs choose between human output and --json with an
  early-return if in every command function, so the two paths drift and
  neither is testable without running the binary. One emit() and a View per
  thing replaces that, and Out can be pointed at a buffer, so a renderer is
  an ordinary unit test.

  View requires Serialize as a supertrait, so the JSON half cannot silently
  diverge from the type it claims to describe. Colour is refused outright
  for JSON: escape codes would make the document unparseable.

  No domain type here and there must never be one -- that is what the crate
  is for. Prompts write to stderr so they are never inside the document
  --json is producing.
