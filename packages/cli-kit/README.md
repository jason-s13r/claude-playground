# cli-kit

Presentation for a command line tool, knowing nothing about what is being
presented.

Output routing, tables, prompts, shell completions and health reports. There is
no domain type in here and there must never be one: a table is a table whether
it holds groceries or anything else.

## The problem it solves

The CLIs this was extracted from decided between human output and `--json` with
an early-return `if` in every command function:

```rust
if json {
    println!("{}", serde_json::to_string(&products)?);
    return Ok(());
}
// ... forty lines of table building
```

Two consequences. The two paths drift, because nothing makes the JSON and the
table describe the same thing. And neither is testable without running the
binary and reading its stdout.

Here a thing that can be shown implements [`View`](src/out.rs), which has a
`text` method and gets `json` for free from `Serialize`. One `emit` chooses
between them, and an [`Out`](src/out.rs) can be pointed at a buffer — so a
renderer is an ordinary unit test.

```rust
use cli_kit::{emit, Format, Out, View};

#[derive(serde::Serialize)]
struct Greeting { name: String }

impl View for Greeting {
    fn text(&self, out: &mut Out) -> std::io::Result<()> {
        let title = out.heading("Hello");
        writeln!(out, "{title}, {}", self.name)
    }
}

let mut out = Out::buffer(Format::Text);
emit(&mut out, &Greeting { name: "world".into() })?;
assert_eq!(out.into_string(), "Hello, world\n");
```

The same value with `Format::Json` serialises the struct instead. Override
`json` only where the wire shape should differ from the type.

`Out::stdout(format, no_color)` takes the caller's reading of `NO_COLOR` and
`--color`, because this crate does not read the environment either. Colour is
off for a pipe and off for JSON, where escape codes would make the document
unparseable.

## What is in it

| Module | What it does |
| ------ | ------------ |
| [`out`](src/out.rs) | `Out`, `Format`, the `View` trait and `emit` — the one place a program chooses its output shape |
| [`table`](src/table.rs) | A `comfy-table` with the house styling, plus `plural` and `qualified` |
| [`io`](src/io.rs) | Prompts, password entry and confirmation, all writing to **stderr** so a prompt is never inside the `--json` document |
| [`doctor`](src/doctor.rs) | `Check` and `Report`: a health report as data, rendered like anything else |
| [`completions`](src/completions.rs) | A shell completion script, with the shell guessed from `$SHELL` where the caller passes it in |

## `doctor` is a report, not a script

A `Report` is a list of `Check`s, each ok, warn, fail or skip, each with a
detail and an optional hint. Nothing aborts the run: a failure early on is
exactly when the later lines are most worth seeing, and a report that stops at
the first problem makes someone fix their setup one round trip at a time.
`healthy()` decides the exit code afterwards.

`comfy_table` and `serde_json` are re-exported for the same reason `net-kit`
re-exports `wreq`: a consumer building rows compiles against the same
`comfy-table` the `table` helper returns.

## Development

```bash
dispat run check --since all -p cli-kit
```

Used by [`gsnz-ui`](../gsnz-ui) and both apps. Not published to crates.io;
consumers declare a path dependency, as [`packages/README.md`](../README.md)
describes.
