# Changelog

All notable changes to Scorium are recorded here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once it reaches 1.0. Until then, the public API may change between minor
versions.

## [Unreleased]

### Added
- `scorium` command-line tool with `check`, `parse`, `fmt`, `fmt --check`,
  and `eval` subcommands.
- `examples/embedding/`, a complete parse + evaluate + validate example that
  registers a host value and a host function and validates against a schema.
- Example `.scor` files: `basic.scor`, `variables.scor`, `conditions.scor`,
  `loops.scor`.
- Project legal and branding files: `LICENSE` (PolyForm Strict 1.0.0),
  `COMMERCIAL.md`, `TRADEMARKS.md`, `CONTRIBUTING.md`,
  `CONTRIBUTION_PERMISSION.md`, `CONTRIBUTOR_TERMS.md`, `SECURITY.md`.
- Project documentation under `docs/`: language guide, grammar, embedding,
  diagnostics, security model, licensing, and roadmap.
- CI workflow running `cargo fmt --check`, `cargo clippy`, `cargo test`, and
  `cargo build` across the workspace.

### Changed
- `scorium-cli` now depends only on the crates and tools it actually uses.

### Notes
- Scorium is source-available under PolyForm Strict 1.0.0 and is **not**
  OSI-approved open source. See `docs/LICENSING.md`.

## [0.1.0] - initial implementation

### Added
- `scorium-core`: source handling, byte-offset spans, lexer, parser, AST,
  typed literal values (integers, floats, booleans, nil, strings, lists,
  colors, durations), and syntax diagnostics rendered with `miette`.
- `scorium-lua`: sandboxed evaluation runtime built on `mlua` (Lua 5.4,
  vendored). The Lua state exposes only `math`, `string`, and `table`.
  Direct statement interpretation for `if`/`elseif`/`else`, numeric `for`,
  `while`, `fn`, `local`, and `return`. `script { }` blocks run as real Lua.
  Variable definition (`@name`), bare-string `$name` interpolation, plain
  identifiers in expressions, sibling-leaf access, color methods
  (`darken`/`lighten`/`alpha`), arithmetic, comparison, and boolean
  operators. Includes with cycle detection and configurable path policy.
  One host registry shared by the expression and `script` surfaces. Loop and
  instruction budgets as sandbox limits.
- `scorium-schema`: builder API for declaring valid nodes and keys, expected
  value types, required keys, duplicate-key policies, unknown-node/key
  handling, and Levenshtein-based typo suggestions. Custom-type extension
  point for host-defined validation.
- `scorium-format`: canonical formatter rendering straight from the AST,
  with idempotent output, four-space indentation, comment preservation
  (leading and one trailing per item), and byte-for-byte `script { }` bodies.

[Unreleased]: https://github.com/fiw-labs/scorium/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/fiw-labs/scorium/releases/tag/v0.1.0
