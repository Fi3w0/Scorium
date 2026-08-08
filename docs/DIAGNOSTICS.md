# Diagnostics

Scorium reports problems through [`miette`](https://docs.rs/miette), so every
diagnostic carries a source excerpt, a caret underline, and an actionable
message. This document catalogues the diagnostic codes the current
implementation emits.

Every code is namespaced as `scorium::<stage>::<name>`.

```
scorium::lex::*       lexer
scorium::parse::*     parser
scorium::eval::*      evaluator (runtime)
scorium::schema::*    schema validation
```

---

## Example

```
config.scor:8:5
unknown key `timeuot`
   ╭─── config.scor:8:5 ───
 8 │     timeuot = 5s
   ·     ───────
   ╰────
help: did you mean `timeout`?
```

A diagnostic includes:

- **path**, **line**, **column**;
- a **source excerpt** with an underline or caret;
- an **actionable explanation**;
- a **related span** where useful (for example the first occurrence of a
  duplicate key).

---

## Lexer (`scorium::lex`)

| Code | Meaning |
| --- | --- |
| `scorium::lex::unexpected_char` | A character that cannot start any token. |
| `scorium::lex::unterminated_string` | A `"..."` literal with no closing quote. Suggests adding `"`. |
| `scorium::lex::unterminated_comment` | A `--[[ ]]` block comment with no closing `]]`. |
| `scorium::lex::squeezed_operator` | An operator glued to an operand (`base*2`). Suggests the spaced form. |

## Parser (`scorium::parse`)

| Code | Meaning |
| --- | --- |
| `scorium::parse::unexpected_token` | The token present cannot continue the current construct. |
| `scorium::parse::at_in_expression` | `@name` used inside an expression. `@` only defines; use `name`. |
| `scorium::parse::dollar_in_expression` | `$name` used inside an expression. Use `name`. |
| `scorium::parse::reserved_word` | A reserved word used as a node name. |
| `scorium::parse::unexpected_eof` | Input ended before a construct was complete. |

## Include (parser-level, `scorium::include`)

| Code | Meaning |
| --- | --- |
| `scorium::include::cycle` | An include chain closed back on itself. |

## Evaluator (`scorium::eval`)

| Code | Meaning |
| --- | --- |
| `scorium::eval::undefined_interpolation` | `$name` in a bare string names no defined variable. Suggests `@name = ...`. |
| `scorium::eval::unknown_function` | A call to a name that is neither a host function nor a Scorium `fn`. |
| `scorium::eval::type_error` | An operand had the wrong type for the operation. |
| `scorium::eval::division_by_zero` | Division or modulo by zero. |
| `scorium::eval::arithmetic_overflow` | Integer arithmetic exceeded the supported 64-bit range. |
| `scorium::eval::includes_disabled` | `include` used while the host disabled includes. |
| `scorium::eval::include_path_denied` | An include path blocked by the host's path policy. |
| `scorium::eval::include_cycle` | An include cycle detected at evaluation time, with the include chain. |
| `scorium::eval::include_io` | An included file could not be read. |
| `scorium::eval::include_parse` | An included file failed to parse. |
| `scorium::eval::script_error` | A `script { }` block raised a Lua error or hit the instruction budget. |
| `scorium::eval::loop_budget_exceeded` | The total loop iteration count exceeded the sandbox limit. |
| `scorium::eval::call_depth_exceeded` | Nested Scorium function calls exceeded the sandbox limit. |

## Schema (`scorium::schema`)

| Code | Meaning |
| --- | --- |
| `scorium::schema::unknown_node` | A node name not declared in the schema. Carries a typo suggestion when one is close. |
| `scorium::schema::unknown_key` | A key not declared for its node. Carries a typo suggestion when one is close. |
| `scorium::schema::wrong_type` | A value whose type does not match the schema. |
| `scorium::schema::missing_required_key` | A node missing a key declared `required`. |
| `scorium::schema::duplicate_key` | A key set twice under a node with an `Error` duplicate policy. Carries the first-occurrence span. |
| `scorium::schema::invalid_header` | A node header rejected by host-supplied header validation. |

---

## Rendering

From Rust, every diagnostic type has a helper that attaches source text for
rendering:

```rust
// syntax errors
let report = scorium_core::diagnostic::with_source(err, &source);
eprintln!("{report:?}");

// evaluation errors (carry their own source)
eprintln!("{:?}", eval_err.report());

// schema errors
for report in schema_result.reports(&source) {
    eprintln!("{report:?}");
}
```

Printing a `miette::Report` with `{report:?}` is what activates the graphical
handler. The standalone CLI does exactly this.
