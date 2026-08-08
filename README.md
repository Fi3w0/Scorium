# Scorium

[![Rust](https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-pre--1.0-8EDDFF?style=flat-square)](./docs/ROADMAP.md)
[![Source Available](https://img.shields.io/badge/source-available-8EDDFF?style=flat-square)](./LICENSE)
[![GitHub Stars](https://img.shields.io/github/stars/Fi3w0/Scorium?style=flat-square&logo=github)](https://github.com/Fi3w0/Scorium/stargazers)
[![GitHub Issues](https://img.shields.io/github/issues/Fi3w0/Scorium?style=flat-square&logo=github)](https://github.com/Fi3w0/Scorium/issues)

**Readable on the surface. Programmable when you need it.**

Scorium is a readable, programmable configuration framework. It keeps
ordinary configuration declarative while allowing Lua-powered expressions,
conditions, loops, and functions when static data is not enough.

> Simple configuration for beginners. Programmable configuration for
> advanced users.

```scor
@base_port = 8000

server {
    host = localhost
    port = base_port + 80
    timeout = 5s
    enabled = true
}

for i = 1, 3 do
    worker {
        name = worker-$i
        index = i
    }
end
```

A beginner writes ordinary data and never touches the programmable layer. An
advanced user adds logic without migrating to another file format.

---

## Why Scorium?

Most configuration formats are easy to read but become repetitive as they
grow. Pure scripting languages remove repetition, but force every user to
write code. Scorium is the middle that does not force the choice: a file
reads like data, and the moment you need a loop, a conditional, a function,
or a raw Lua block, the language is already there.

- **Readable on the surface.** Nodes, `key = value` leaves, `@name = value`
  variables, `$name` references in bare strings, quoted strings that mean
  what they say. A config file is readable by someone who has never seen the
  language.
- **Programmable when needed.** `if`, `for`, `while`, `fn`, `include`, and
  `script { }` blocks with raw Lua. Loops generate repeated structure.
  Conditionals branch on values. Functions encode your conventions once.
- **Typed values.** Colors and durations are real values, not strings:
  `#8EDDFF` carries RGBA channels, so `primary.darken(0.35)` derives a
  palette from one line; `600ms` is a duration, not text.
- **Validation with suggestions.** A schema is the set of nodes and keys your
  application accepts. Unknown keys and wrong types fail with precise spans
  and typo suggestions, instead of silently becoming strings.
- **Sandboxed by design.** Evaluation runs against a restricted Lua state
  (`math`, `string`, `table` only) with bounded loops and instruction
  budgets. No `io`, no `os`, no network, no processes -- unless the host
  explicitly grants them.
- **Canonical formatting.** `scorium fmt` renders canonical output straight
  from the syntax tree, idempotent by construction.
- **Embeddable.** The runtime is a Rust library: lexer, parser, AST,
  evaluator, schema, formatter. Your application defines the host functions
  and the schema; Scorium does the rest.

## How it differs from JSON, YAML, TOML, and raw Lua

JSON is suitable for machine interchange. Scorium is designed for
human-written application configuration -- it is not trying to replace JSON
as a serialization format.

| | JSON | YAML | TOML | Raw Lua | **Scorium** |
| --- | --- | --- | --- | --- | --- |
| Human-readable config | verbose | yes | yes | verbose | **yes** |
| Typed literals (color, duration) | no | no | no | no | **yes** |
| Logic when needed | no | anchors only | no | always | **opt-in** |
| Beginners write pure data | n/a | yes | yes | no | **yes** |
| Sandboxed | n/a | n/a | n/a | no | **yes** |
| Schema + typo suggestions | external | external | no | no | **built-in** |

## A first file

```scor
server {
    port = 8080
    timeout = 5s
    enabled = true
}
```

A more advanced one, using a variable, interpolation, an expression, a
condition, and a function:

```scor
@mod = SUPER
@base = 8

binding = $mod+Return
gaps = base * 2

theme {
    primary = #8EDDFF
    deep = primary.darken(0.35)
}

if gpu == nvidia then
    driver {
        overlay_planes = false
    }
end

fn service(name, port) {
    server {
        id = $name
        port = port
    }
}
service(web, 8080)
```

Three rules cover variables almost completely:

| Where | Form | Meaning |
| --- | --- | --- |
| Definition | `@name = value` | Defines a variable. `@` appears only here. |
| In a bare string | `$name` | Interpolates the value into the string. |
| In an expression | `name` | References the typed value. |

Read the [language guide](./docs/LANGUAGE.md) for the rest.

## Install and use

Build the CLI from source (Scorium is not published to a package registry --
see [Licensing](#licensing)):

```bash
cargo build -p scorium-cli
# then, from the repository root:
cargo run -p scorium-cli -- check examples/basic.scor
cargo run -p scorium-cli -- fmt --check examples/basic.scor
cargo run -p scorium-cli -- parse examples/variables.scor
cargo run -p scorium-cli -- eval examples/conditions.scor
```

| Command | Does |
| --- | --- |
| `scorium check file.scor` | Parse + evaluate; report diagnostics. |
| `scorium parse file.scor` | Print the parsed syntax tree. |
| `scorium fmt file.scor` | Format a file in place. |
| `scorium fmt --check file.scor` | Exit non-zero if a file isn't formatted. |
| `scorium eval file.scor` | Print the evaluated configuration tree. |

`check` and `eval` run against a **generic** runtime: control flow,
variables, arithmetic, includes, and `script { }` all work without a host,
but host-registered functions and schema validation require an embedding
application. See the [embedding example](./examples/embedding/).

## Embed it

Scorium is a Rust workspace of focused crates:

| Crate | What it is |
| --- | --- |
| [`scorium-core`](./crates/scorium-core) | Lexer, parser, AST, typed values, spans, diagnostics. |
| [`scorium-lua`](./crates/scorium-lua) | Sandboxed evaluator, control flow, includes, host registry. |
| [`scorium-schema`](./crates/scorium-schema) | Schema builder, validation, typo suggestions, custom types. |
| [`scorium-format`](./crates/scorium-format) | Canonical formatter. |
| [`scorium-cli`](./crates/scorium-cli) | The `scorium` command-line tool. |

A complete embedding -- parse, evaluate against a host runtime, validate
against a schema, inspect -- is in
[`examples/embedding/`](./examples/embedding/):

```rust
use scorium_core::{parse, Source, Value};
use scorium_lua::{Runtime, RuntimeOptions};
use scorium_schema::{NodeSchema, Schema, ValueType};

// 1. parse, 2. evaluate with registered host value + function,
// 3. validate against a schema, 4. inspect.
```

Run it with `cargo run -p scorium-embedding-example`. Full API documentation
is in [docs/EMBEDDING.md](./docs/EMBEDDING.md).

## Status

The language core, the sandboxed evaluator, the schema validator, the
canonical formatter, and the CLI are implemented and tested. This is a real,
compiling, embeddable foundation, but it is pre-1.0 and the public API may
change. See [docs/ROADMAP.md](./docs/ROADMAP.md) for what is planned and what
is deferred.

## Documentation

- [Language guide](./docs/LANGUAGE.md) -- start here.
- [Grammar](./docs/GRAMMAR.md) -- the implemented grammar.
- [Embedding](./docs/EMBEDDING.md) -- the Rust API for hosts.
- [Diagnostics](./docs/DIAGNOSTICS.md) -- the diagnostic catalogue.
- [Security model](./docs/SECURITY.md) -- the sandbox and host responsibility.
- [Roadmap](./docs/ROADMAP.md) -- what exists and what is deferred.

## Security

`script { }` blocks run against a restricted Lua state (`math`, `string`,
`table` only). There is no `io`, `os`, `package`, `debug`, process spawning,
filesystem access, or networking. Loops and Lua instructions are bounded. See
[docs/SECURITY.md](./docs/SECURITY.md) and report vulnerabilities privately
as described in [SECURITY.md](./SECURITY.md).

## Licensing

Scorium is **source-available** under the
[PolyForm Strict License 1.0.0](./LICENSE). It is free for personal,
educational, hobby, and local noncommercial use, and for contribution-focused
forks. **Commercial use requires a written agreement.**

Scorium is *not* OSI-approved open source, and is not published to a package
registry. See [docs/LICENSING.md](./docs/LICENSING.md),
[COMMERCIAL.md](./COMMERCIAL.md), and [TRADEMARKS.md](./TRADEMARKS.md).

> The legal files are initial project terms that have not been reviewed by a
> lawyer. Obtain professional legal review before relying on them for
> commercial use.

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](./CONTRIBUTING.md),
[CONTRIBUTION_PERMISSION.md](./CONTRIBUTION_PERMISSION.md), and
[CONTRIBUTOR_TERMS.md](./CONTRIBUTOR_TERMS.md) before opening a pull request.
