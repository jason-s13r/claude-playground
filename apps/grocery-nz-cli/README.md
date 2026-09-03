# grocery-nz-cli

`gsnz` — New World, PAK'nSAVE and Woolworths NZ from one command line.

```console
$ gsnz compare "2l milk"
```

Unofficial. Everything here is reverse-engineered from what the three websites'
own frontends call, so treat a missing column as a field that was renamed rather
than as a product that does not exist.

## Why a third CLI

[`fsnz`](../foodstuffs-nz-cli) speaks to Foodstuffs and
[`wwnz`](../woolworths-nz-cli) to Woolworths, and neither can answer the obvious
question: *is it cheaper at the other one?* `gsnz` puts all three side by side,
and targets one with `-b`.

It is built on seven libraries in [`packages/`](../../packages), so the shared
half — HTTP that is not scored as a bot, credentials, the self-update, the
renderers — has one implementation rather than two drifting copies. The two
existing CLIs are untouched; moving them onto these libraries is separate work.

## Install

```bash
cargo install --path .
gsnz update            # afterwards, replaces itself from GitHub releases
```

## Getting started

```bash
gsnz -b nw store set thorndon      # pick a shop and a store, remembered
gsnz search "weetbix"
gsnz specials --limit 10
gsnz compare "2l milk"             # all three, side by side

gsnz -b ww auth login              # a cart and orders need an account
gsnz -b ww cart add 282768 2
gsnz -b ww orders list
```

`gsnz doctor` prints what is set up and then checks it: one call per shop, so
it reports whether the thing works rather than only whether it is configured.
It exits non-zero when a shop cannot be reached. Being signed out is not a
fault -- most of this tool works signed out.

## Commands

```
gsnz [-b nw|pns|ww] [--store ID] [--token T] [--json]
├── search <QUERY>        [--limit --size --sort --specials]
├── specials              [--limit --size --sort]
├── browse <DEPARTMENT>   [--limit --size --sort --specials]
├── departments [QUERY]   [--depth]
├── compare <QUERY>       [--limit --size --sort --specials --strict]
├── stores [QUERY]        [--limit]
├── use [SHOP]
├── config  list | get <KEY> | set <KEY> <VALUE> | unset <KEY> | path
├── store   show | set <STORE> | clear
├── cart    list | add <SKU> [QTY] [--unit kg] | update <SKU> <QTY>
│                | remove <SKU> | clear --force
├── orders  list [--limit --filter] | show <POSITION_OR_ID>
│                | previous [--limit --include-cart]
├── auth    login [--email --password-command --no-store-password]
│                | import <COOKIES_FILE> | refresh | logout | status
├── doctor
├── completions [SHELL]
└── update [VERSION] [--check --pre-release]
```

`-b` takes a list for `compare` (`-b nw,pns`) and exactly one shop everywhere
else.

## Reading a comparison

New World and PAK'nSAVE share one Foodstuffs catalogue, so their rows are joined
on the product code and are exact. Woolworths has its own codes, so it is
attached by brand, name and canonicalised size instead — `2L`, `2 litre` and
`2000ml` all fold to the same thing.

**Those rows are marked with `~`, and the marker matters.** A table that
silently equates two different two-litre milks is a wrong-price bug, which is
the worst kind this tool can have. `--strict` drops them; `--json` carries
`"match": "normalised"` on each.

`gsnz --version` prints the whole provenance -- commit, source, toolchain, how
this file got installed -- and the version of each of the seven libraries it
was built against. They release on their own tags, so "gsnz 0.1.0" alone does
not say which `fsnz-api` is compiled in, and that is the part that breaks when
a supermarket changes its API. `gsnz -V` stays one line.

## Exit codes

A wrapper should not have to read stderr to know what happened.

| | |
|---|---|
| 0 | it worked |
| 1 | something upstream failed |
| 2 | the command was wrong |
| 3 | sign in, or sign in again |
| 4 | this shop cannot do that |
| 5 | no store selected |

## Sessions

Three shops, two logins. One Club Plus account covers both Foodstuffs banners;
Woolworths is separate.

```bash
gsnz auth login        # both accounts, two prompts -- the whole setup
```

Every `auth` command works in those units rather than per shop, and names what
it covered: signing in as `-b nw` signs in PAK'nSAVE, and signing out of either
signs out of both. That is why there is no `-b fs`; with no `-b` at all, `auth
login` already asks once per account.

- **Foodstuffs** renews itself from a rotating refresh token, so a login lasts
  well past its half-hour access token. `auth import` seeds one from a browser's
  `cookies.txt` — bring `refresh_token` as well as `fs-user-token`, or the
  imported session lapses within the hour with no way to renew.
- **Woolworths** cannot be renewed at all: the session cookie is encrypted and
  only the site can mint one. `auth refresh` therefore walks the whole login
  flow again, which needs the password — kept at login unless
  `--no-store-password`, or supplied by `password_command`.

Credentials go to the platform credential store, or to a 0600 file where there
is none. `gsnz` has its own namespace and does not read `fsnz` or `wwnz`'s.

## Configuration

`~/.config/grocery-nz-cli/config.toml`:

```toml
retailer = "nw"

[compare]
retailers = ["nw", "pns", "ww"]
match = "normalised"        # or "exact"

[auth]
password_command = "pass show groceries"
store_password = true

[output]
color = "auto"              # auto | always | never

[newworld]
store_id = "..."
token_command = "..."       # Foodstuffs only

[woolworths]
store_id = "..."
```

Precedence is flag, then environment, then this file, then the default. An
unknown key is an error rather than a setting that silently does nothing.

Nothing here has to be edited by hand:

```bash
gsnz use ww                              # the default shop
gsnz config list                         # every setting, and what it does
gsnz config set compare.retailers nw,ww
gsnz config unset auth.password_command
```

A value is parsed before it is written, so a typo is refused at the point of
making it rather than by the next command that reads it, and only settings that
differ from their default are kept in the file. `store set` stays its own
command because it is not a plain write: it resolves a name against the live
store list, and on Woolworths it binds the cart server-side.

### Environment

`GSNZ_CONFIG_DIR`, `GSNZ_STATE_DIR`, `GSNZ_RETAILER`, `GSNZ_TOKEN`,
`GSNZ_SECRET_BACKEND`, `GSNZ_UPDATE_API`, `GSNZ_DEBUG_AUTH`,
`GSNZ_{NEWWORLD,PAKNSAVE}_{ORIGIN,API,STORE_ID,TOKEN}`,
`GSNZ_WOOLWORTHS_{ORIGIN,AUTH_ORIGIN,STORE_ID}`, `GSNZ_CLUBPLUS_{ORIGIN,API}`,
plus `NO_COLOR`, `GITHUB_TOKEN` and `GH_TOKEN`.

The origin overrides are escape hatches, mainly so the test suite can point the
binary at a mock server. `src/env.rs` is the only place in the whole tree that
reads any of them: the libraries take values, and a `clippy.toml` in each
enforces it.

## Development

```bash
dispat run check --since all              # what CI runs
dispat run test  --since all -p grocery-nz-cli
```

No test touches the network. The two login chains are the exception that cannot
be covered that way: `auth login` against real Club Plus and real Auth0 was
verified by hand at v0.1.0, and the tests here do not stand in for repeating
that whenever either flow changes.
