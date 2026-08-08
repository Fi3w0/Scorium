# Embedding Scorium

Scorium is a Rust library first and a CLI second. This document shows how an
application embeds it: parse, evaluate, validate, and inspect a `.scor`
configuration.

A runnable version of everything below lives at
[`examples/embedding/`](../examples/embedding/).

---

## The pipeline

Scorium is intentionally staged so a host can stop at any point:

```
source text
  -> lex                  (scorium-core)
  -> parse -> AST         (scorium-core)
  -> evaluate -> entries  (scorium-lua, sandboxed)
  -> validate             (scorium-schema, against a host schema)
  -> inspect / apply      (host decides)
```

The host applies configuration only after evaluation and validation
succeed. Scorium never mutates application state itself.

---

## 1. Parse

```rust
use scorium_core::{Source, parse};

let source = Source::from_path("config.scor")?;
// or, for in-memory input:
// let source = Source::new("<inline>", text);

let doc = match parse(&source) {
    Ok(doc) => doc,
    Err(err) => {
        // `miette` renders the syntax error with a source excerpt + caret.
        eprintln!("{:?}", scorium_core::diagnostic::with_source(err, &source));
        return Ok(());
    }
};
```

`Document` is the AST (`scorium_core::ast`). Parsing never touches the
filesystem beyond what `Source::from_path` already did.

---

## 2. Evaluate

```rust
use scorium_lua::{Runtime, RuntimeOptions};
use std::path::Path;

let runtime = Runtime::with_options(RuntimeOptions::default())?;
let output = runtime.evaluate(&doc, &source, Path::new("./"))?;
```

`evaluate` returns an `EvalOutput`:

```rust
pub struct EvalOutput {
    pub entries: Vec<Entry>,
    pub warnings: Vec<String>,
}
```

`Entry` is the evaluated configuration tree:

```rust
pub enum Entry {
    Leaf(LeafEntry),         // key = value
    Node(NodeEntry),         // name { children }
    Include(IncludeEntry),   // include "path"
    HostCall(HostCallEntry), // a host function used as a statement
}
```

The second argument to `evaluate` is the `Source` (used for diagnostics when
an error originates in this file), and the third is the **base directory**
relative `include "..."` paths resolve from -- typically the directory
containing the source file.

### Evaluation errors

`EvalError` carries its own source, because with includes an error can
originate in a file other than the one you started with:

```rust
match runtime.evaluate(&doc, &source, base_dir) {
    Ok(output) => { /* ... */ }
    Err(err) => eprintln!("{:?}", err.report()), // attaches the right source
}
```

---

## 3. Register host capabilities

Before evaluating, register the values and functions your application wants
config files to reach. All registration goes through one
[`Runtime`](https://docs.rs) -- the "one registry, multiple surfaces" model:

```rust
use scorium_core::Value;

// A plain identifier in expressions resolves to this value.
runtime.register_value("environment", Value::Str("production".into()));

// Callable from expressions AND from `script { }` blocks alike.
runtime.register_function("double", |args| match args.first() {
    Some(Value::Int(n)) => Ok(Value::Int(n * 2)),
    Some(Value::Float(f)) => Ok(Value::Float(f * 2.0)),
    _ => Err("double() expects one number".to_string()),
});

// A function implemented in Lua itself can also be registered through the
// same registry. Create it in this Runtime's restricted Lua state (values
// cannot cross between independent mlua states):
let lua_double = runtime.lua().create_function(|_, n: i64| Ok(n * 2))?;
runtime.register_lua_function("lua_double", lua_double);
// This is an advanced path; most hosts only need the two methods above.
```

`register_value` and `register_function` cover the vast majority of hosts.
A host that wants declarative **sugar** over a registered function (for
example, turning a `bind { }` node into a `bind()` call) builds that itself
on top of the evaluated tree -- Scorium's node/leaf grammar is just data.

> Note: every registered function is a **capability** you are granting the
> config file. Register only functions you would be comfortable running with
> the file author's intent. See [SECURITY.md](./SECURITY.md).

---

## 4. Validate against a schema

```rust
use scorium_schema::{Schema, NodeSchema, ValueType};

let schema = Schema::builder()
    .node(
        "server",
        NodeSchema::builder()
            .required_key("host", ValueType::String)
            .required_key("port", ValueType::Integer)
            .key("timeout", ValueType::Duration)
            .key("enabled", ValueType::Boolean)
            .build(),
    )
    .build();

let result = schema.validate(&output.entries);
if result.is_valid() {
    println!("configuration is valid");
} else {
    for report in result.reports(&source) {
        eprintln!("{report:?}");
    }
}
```

`validate` collects **every** problem (not just the first). Each
`SchemaErrorKind` renders through `miette` with a source excerpt, a caret,
and -- for unknown nodes and keys -- a Levenshtein-based typo suggestion.

### Built-in value types

`ValueType::{ String, Integer, Float, Boolean, Color, Duration, Any }`.
`Float` accepts integers too (they are promoted). `List(Box<ValueType>)`
checks each element. `Any` accepts any typed value.

### Custom host-defined types

A host can add a validation type that runs its own logic on an already-parsed
`Value`:

```rust
use scorium_core::Value;
use scorium_schema::{CustomType, ValueType};

#[derive(Debug)]
struct Percentage;

impl CustomType for Percentage {
    fn name(&self) -> &str { "percentage" }
    fn validate(&self, value: &Value) -> Result<Value, String> {
        match value {
            Value::Int(n) if (0..=100).contains(n) => Ok(Value::Int(*n)),
            Value::Float(f) if (0.0..=100.0).contains(f) => Ok(Value::Float(*f)),
            other => Err(format!("expected a percentage 0..=100, found {}", other.type_name())),
        }
    }
}

let schema = Schema::builder()
    .key("opacity", ValueType::Custom(std::rc::Rc::new(Percentage)))
    .build();
```

> Scope note. A `CustomType` validates a **value that already parsed** as one
> of Scorium's core literals -- it does not add new lexer syntax. Bespoke
> literal *syntax* (a token shape the lexer parses directly for a host) is
> deferred; see ROADMAP.md.

### Duplicate-key policy

```rust
use scorium_schema::DuplicateKeyPolicy;

NodeSchema::builder()
    .key("port", ValueType::Integer)
    .duplicate_key_policy(DuplicateKeyPolicy::Error) // default
    // or LastWins, or FirstWins
    .build()
```

---

## 5. Format

`scorium-format` renders canonical output straight from the AST, so it is
idempotent (`format(format(x)) == format(x)`) by construction:

```rust
use scorium_format::{format, FormatOptions};

let canonical = format(&doc);                       // default 4-space indent
let wide = format_with(&doc, &FormatOptions { indent_width: 2 });
```

`script { }` bodies are reproduced byte-for-byte (this crate does not
understand Lua syntax). Comment preservation is documented in
[LANGUAGE.md](./LANGUAGE.md#comments).

---

## Runtime options and the sandbox

```rust
use scorium_lua::{RuntimeOptions, IncludePolicy};

let options = RuntimeOptions {
    include_policy: IncludePolicy {
        enabled: true,
        allow_parent_traversal: false, // denies absolute, `..`, and symlink escapes
    },
    max_loop_iterations: 1_000_000,      // total loop iters per evaluation
    max_function_call_depth: 256,        // nested Scorium `fn` calls
    max_script_instructions: 50_000_000, // Lua VM instrs per script block
    max_lua_memory_bytes: 64 * 1024 * 1024, // Lua-owned memory per runtime
};
```

The Lua state is opened with `math`, `string`, and `table` only. There is no
`io`, `os`, `package`, `debug`, process spawning, filesystem access, or
networking. See [SECURITY.md](./SECURITY.md).

---

## Safe updates (transactional reload)

Scorium's staged pipeline lets a host implement transactional reload without
Scorium doing anything application-specific:

1. Parse and evaluate the **new** configuration.
2. Validate it completely.
3. Keep the **old** configuration active if the new file fails at any stage.
4. Compare old and new evaluated `Entry` trees (they implement `PartialEq`).
5. Apply only after successful validation.

Scorium does not implement host-specific hot reload, but the evaluated result
is designed for it: `Entry`, `LeafEntry`, `NodeEntry`, and the `Value` types
are `Clone + PartialEq`, carry source spans, and serialize to a stable tree.

---

## API surface at a glance

| Crate | Entry points |
| --- | --- |
| `scorium-core` | `Source`, `parse`, `Document`, `Entry`, `Value`, `Span`, `SyntaxError` |
| `scorium-lua` | `Runtime`, `RuntimeOptions`, `IncludePolicy`, `EvalOutput`, `Registry`, `EvalError` |
| `scorium-schema` | `Schema`, `NodeSchema`, `ValueType`, `CustomType`, `DuplicateKeyPolicy`, `ValidationResult` |
| `scorium-format` | `format`, `format_with`, `FormatOptions` |

Every public item carries a doc comment; the source is the reference.
