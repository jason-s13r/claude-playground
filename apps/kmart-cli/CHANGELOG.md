# Changelog

## kmart-cli/v0.1.0 (2026-09-05)

### Features

- default to headless, add --direct sign-in
  Headless passes the bot check in the common case, so it is the default;
  `--headful` shows the window and is the stronger path when a headless
  run is refused. `--direct` signs in with no browser at all, replaying
  Auth0's login as direct requests and seeding any admission the session
  holds -- a diagnostic that measures the Akamai wall and would notice the
  day it moves, not a way in. Refresh the now-stale "headless was refused"
  notes to match.

- add kmart, for the Australian and New Zealand stores
  Command-for-command what `twlnz` does, over `kmart-api`. One binary for
  both countries because they share a backend: `kmart use au` sets which
  one, `--country` overrides it for a command.

  `auth login` drives camoufox, because Akamai guards the password submit
  and refuses every HTTP client. It navigates the way a person would --
  home page, then the account link -- since asking for a protected path
  outright is answered with an interstitial that never resolves, and it
  reads the refresh token out of the storefront's own storage rather than
  running a second copy of the OAuth flow. `auth token` and `auth import`
  remain for when there is no browser to hand.

  Wishlist removal is missing: the gateway's add is a bespoke field rather
  than an action list, introspection is off, and no capture of a removal
  exists to name the operation.

  Vendor identifiers are re-read from the storefront about weekly and cached in
  the state directory, falling back to what shipped when it cannot be reached;
  `doctor` reports which is in force.

### Dependencies

- kmart-api: 0.0.0 -> 0.1.0
