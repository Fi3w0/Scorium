# Scorium

**Built on molten Lua.**

**Readable on the surface. Programmable at the core.**

## Why Scorium?

**Readable data. Programmable when needed.**

Most configuration formats are easy to read but become repetitive as they grow. Pure scripting languages remove repetition, but force every user to write code. Scorium is the middle that does not force the choice: a file reads like data, and the moment you need a loop, a conditional, a function, or a raw Lua block, the language is already there, underneath the surface.

```scorium
@mod = SUPER

terminal = select(kitty, alacritty, foot)

theme {
    primary = #8EDDFF
    deep    = primary.darken(0.35)
}

gaps = 8
cursor_hide_after = 2s
```

Nothing here requires programming. Everything here is one level from it.

```scorium
# the same nine settings, written once
for i = 1, 9 do
    title = Item $i
end

if gpu == "nvidia" then
    driver {
        overlay_planes = false
    }
end

fn window_app(id, title) {
    rule {
        app_id = id
        title  = title
        float  = true
    }
}
window_app(pavucontrol, Volume)

script {
    local n = 0
    for k, v in pairs(widgets) do
        n = n + 1
    end
}
```

## What Scorium gives you

- **Readable on the surface.** Nodes, `key = value` leaves, `@name = value` variables, `$name` references in bare strings, quoted strings that mean what they say. A config file is readable by someone who has never seen the language.
- **Programmable at the core.** `if`, `for`, `while`, `fn`, `include`, and `script { }` blocks with raw Lua. Loops generate nine of anything. Conditionals branch on the machine. Macros encode your conventions once.
- **Typed values.** Colors and durations are real values, not strings: `600ms * 2` is `1200ms`, `#8EDDFF` carries channels, so `primary.darken(0.35)` and `primary.lighten(0.15)` derive a palette from one line.
- **Validation with suggestions.** A schema is the set of nodes and keys your application accepts. Unknown keys and wrong types fail with precise spans and typo suggestions, instead of silently becoming strings.
- **Deterministic by design.** Evaluation is sandboxed, no io or os, no network. The same file produces the same result every time, which is what makes hot reload safe: a new version can be diffed against the running one and only the changes applied.
- **Canonical formatting.** `scorium-format` renders the canonical form straight from the syntax tree, idempotent by construction, comments preserved, script bodies untouched.
- **Diagnostics with spans.** Every error knows exactly where it is.
- **Embeddable.** The runtime is a Rust library: lexer, parser, AST, evaluator, schema, formatter. Your application defines the host functions and the schema; Scorium does the rest.

## Where Scorium comes from

Scorium is the tool-agnostic home of the configuration format TideWM calls Wave. The name Wave was already taken by another language, so the standalone version got its own: same grammar, own crates, usable in any application that would otherwise reach for YAML or JSON.

## Status

A workspace of focused crates:

| Crate | What it is |
| --- | --- |
| `scorium-core` | Lexer, parser, AST, typed values, spans, diagnostics |
| `scorium-lua` | Sandboxed evaluator, control flow, includes, host registry |
| `scorium-schema` | Schema builder, validation, typo suggestions |
| `scorium-format` | Canonical formatter |
| `scorium-cli` | The command-line front end |

core, lua, and schema are tested and passing. The formatter printer, the CLI, and the language docs are the open work.

## License

PolyForm Strict 1.0.0. Source-available: you can read it, run it, and audit it; commercial use and redistribution are governed by the license.
