//! `scorium-format`: the canonical `.scor` formatter.
//!
//! Renders straight from the AST rather than re-flowing the original
//! text, so the output only depends on what the parser saw as
//! structure -- not on the input's spacing. That's what makes
//! `format(format(x)) == format(x)`: the same AST always prints the same
//! text, so the only way idempotency could fail is if canonical text
//! re-parsed into a *different* AST than the one that produced it, which
//! the fixture-driven idempotency tests check directly. `script { }`
//! bodies are the one exception -- their raw text is reproduced
//! byte-for-byte, never reformatted, since this crate doesn't understand
//! Lua syntax.
//!
//! Comments are preserved (never silently dropped) at the granularity
//! the parser tracks them: leading comments on their own line(s) above
//! an item, and one trailing comment on the same line after it. A
//! comment written *inside* an expression, list, or call's parentheses
//! isn't tracked by the AST at all and is therefore lost -- documented
//! as a known limitation in `docs/LANGUAGE.md`. Line comments (`#` and
//! `--`) are normalized to `#`; block comments (`--[[ ]]`) are kept as
//! written.

mod printer;

pub use printer::{format, format_with, FormatOptions};
