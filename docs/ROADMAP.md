# Roadmap

What Scorium does today, and what is planned. This document separates
**implemented** from **deferred** so it does not advertise features that do
not exist yet.

---

## Status

The language core, the sandboxed evaluator, the schema validator, and the
canonical formatter are implemented and tested. The CLI (`check`, `parse`,
`fmt`, `eval`) ships. This is a real, compiling, embeddable foundation -- but
it is pre-1.0, and the public API may change between minor versions.

---

## Implemented

### Language
- Nodes (with optional headers), leaves, nested nodes.
- Typed values: integers, floats, booleans, nil, bare/quoted strings, lists,
  colors (`#RRGGBB` / `#RRGGBBAA`), durations (`ms`/`s`/`m`).
- Variables: `@name` definition, `$name` bare-string interpolation, plain
  `name` references, sibling-leaf access in a node body.
- Expressions: arithmetic, comparison, boolean, unary; function calls; color
  methods (`darken`/`lighten`/`alpha`).
- Control flow: `if`/`elseif`/`else`, numeric `for` (with optional step),
  `while`, `local`, `return`, `fn`.
- `script { }` raw Lua under the sandbox.
- `include` with cycle detection and host path policy.
- Comments: `#`, `--`, and `--[[ ]]` block comments.

### Tooling
- `scorium` CLI: `check`, `parse`, `fmt`, `fmt --check`, `eval`.
- Canonical formatter, idempotent by construction, comment-preserving (at
  the documented granularity).
- Diagnostics through `miette`: spans, carets, typo suggestions.

### Integration
- `scorium-core` (parse, AST, values), `scorium-lua` (sandboxed runtime),
  `scorium-schema` (validation), `scorium-format` (formatter).
- Host value and host function registration through one shared registry.
- Host-defined custom validation types (`CustomType`).
- Runtime options for loop budget, instruction budget, and include policy.

---

## Deferred

These are not in this version. They are intended directions, not promises.

### Language
- **Host-pluggable literal *syntax*.** Today a host can add a `CustomType`
  that *validates* an already-parsed value; it cannot add a new token shape
  the lexer parses directly (for example a bespoke `10MB` byte-size literal).
- **Generic `for` over tables/iterators.** Only the numeric `for` is
  supported at the statement level. (`script { }` blocks can still use
  Lua's full generic `for` against `math`/`string`/`table`.)
- **More string escapes** (`\u{...}`, `\xNN`).
- **Nested block comments** and comment tracking inside expressions/lists.

### Schema
- **Schema file format.** Schemas are built in Rust today. A `.scor`-based
  schema language is deliberately deferred until there is a concrete need for
  one -- inventing an unstable schema language without a use case is not
  worth the churn.
- **Header validation helpers** beyond the host-defined `CustomType` path.

### Tooling
- **LSP / language server** (hover, completion, go-to-definition, diagnostics
  in the editor).
- **JSON output** for `parse` and `eval` (today they print human-readable
  trees).
- **A diff-based `fmt --check`** that shows what would change, not just that
  it would.
- **Snapshot / golden tests harness** shared across crates.

### Ecosystem
- **More host functions** is the host's job, not Scorium's; a small
  optional "standard host function pack" (string formatting, math helpers)
  could be offered as a separate crate.
- **Versioning and migration guidance** once the API stabilises past 1.0.

---

## Non-goals

Scorium is deliberately **not** trying to be:

- a general-purpose programming language;
- a replacement for JSON as a machine-interchange format;
- plain Lua with cosmetic syntax;
- tied to any single application, compositor, desktop, or operating system.

If a feature would push Scorium toward one of these, it probably does not
belong in the core.
