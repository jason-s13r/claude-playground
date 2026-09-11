# net-kit

The process boundary: HTTP that is not scored as a bot, cookies that survive a
run, credentials that are not a plaintext file, and the paths those live under.

## This crate does not read the environment

Every entry point takes a value. Nothing calls `std::env::var`, and
[`clippy.toml`](clippy.toml) fails the build over it. A variable name read
inside a library is a variable name every consumer is stuck with, and the apps
have different prefixes for the same setting. The app reads its environment
once, at the top, and passes the results down.

## What is in it

| Module | What it does |
| ------ | ------------ |
| [`http`](src/http.rs) | Building a `wreq` client from a `ClientSpec`, and reading JSON or text off a response with the failure detail kept |
| [`cookies`](src/cookies.rs) | A `Jar` that persists between runs, filtered by the caller's `keep` predicate, plus Netscape `cookies.txt` import |
| [`secrets`](src/secrets.rs) | The OS credential store via `keyring`, falling back to a 0600 file where there is none |
| [`password`](src/password.rs) | A password kept beside a session so a lapsed login can be renewed unattended, and the `Source` that decides where it comes from |
| [`paths`](src/paths.rs) | Config and state directories, platform defaults, per-retailer namespacing, and `restrict` for owner-only files |
| [`config`](src/config.rs) | TOML config load and save; a missing file is the default config, not an error |
| [`jwt`](src/jwt.rs) | Reading claims and expiries out of a token. Verifies nothing — it reports what the issuer said |
| [`run`](src/run.rs) | Running a `password_command` and taking its stdout |
| [`error`](src/error.rs) | Failures with their evidence attached: an `HttpError` keeps its status and body |

## Using it

```rust
use net_kit::{http, Backend, Jar, Paths, Secrets};

// The app resolved these from its own environment variables already.
let paths = Paths::defaults("grocery-nz-cli")?.with_state_dir(state_override);
let secrets = Secrets::new("grocery-nz-cli", Backend::detect(), &paths.state_dir);

let jar = std::sync::Arc::new(Jar::load(&secrets, "newworld", vendor::cookie_keep));
let client = http::build(vendor::client_spec(jar.clone()))?;
```

`Paths::scoped` keeps one tool's several accounts apart. A New World token
presented with a PAK'nSAVE store is not refused — it answers with an empty cart
belonging to nobody.

## `wreq`, not `reqwest`

The storefronts sit behind Cloudflare and Akamai, which fingerprint the TLS
handshake and the HTTP/2 settings rather than the headers. Every `reqwest` TLS
backend is scored as a bot and answered with a bare 400 or a challenge page.
`wreq` presents a real browser's fingerprint.

Two consequences:

- **Do not add `http1_only()`.** A browser fingerprint speaking HTTP/1.1 is
  itself inconsistent, and gets challenged.
- **`ClientSpec` has no `Default`**, and `profile` and `redirect` are required
  arguments. Foodstuffs needs a cookie jar and followed redirects; Woolworths
  needs neither, so that an unexpected redirect surfaces as the bot check it
  is.

`wreq` and `wreq_util` are re-exported. Several crates here name `wreq::Client`
in their signatures and each has its own lockfile; two of them resolving
different majors would surface as a trait mismatch rather than a version error,
so they depend on `net_kit::wreq` instead of listing `wreq` themselves.

## Cookies are credentials

The jar is stored *in* the credential store, not beside it: the cookies worth
keeping between runs are the bot-manager clearance and the session, and a cold
start is scored as a new visitor. Analytics and UI state are dropped by the
caller's `keep` predicate, and session cookies (no expiry) are dropped at exit,
as a browser does.

## Development

```bash
dispat run check --since all -p net-kit
```

Unit tests beside the code, none touching the network: paths, cookie parsing,
JWT claims, the config round trip and the secret backends against `tempfile`.
Wire behaviour is tested in the crates that own a wire.

Used by every app and every API crate here. Not published to crates.io;
consumers declare a path dependency.
