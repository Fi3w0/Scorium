# The Scorium Language

Scorium is a readable, programmable configuration framework. It keeps
ordinary configuration declarative while allowing Lua-powered expressions,
conditions, loops, and functions when static data is not enough.

> Simple configuration for beginners. Programmable configuration for
> advanced users.

A beginner can write ordinary configuration without knowing anything about
programming:

```scor
server {
    port = 8080
    timeout = 5s
    enabled = true
}
```

An advanced user can remove repetition and add logic without migrating to
another file format:

```scor
@base_port = 8000

for i = 1, 3 do
    server {
        port = base_port + i
        name = node-$i
    }
end
```

This guide introduces the language progressively. A beginner can stop after
the first three sections (nodes and leaves, values, variables).

---

## Contents

1. [Nodes and leaves](#1-nodes-and-leaves)
2. [Values](#2-values)
3. [Variables](#3-variables)
4. [Expressions](#4-expressions)
5. [Conditions](#5-conditions)
6. [Loops](#6-loops)
7. [Functions](#7-functions)
8. [Includes](#8-includes)
9. [Scripts](#9-scripts)
10. [Host APIs](#10-host-apis)
11. [Security limitations](#11-security-limitations)

---

## 1. Nodes and leaves

A Scorium file is a list of **items**. The two basic items are *nodes* and
*leaves*.

A **leaf** assigns a typed value to a key:

```scor
port = 8080
enabled = true
name = example
```

A **node** groups configuration under a name using `{ }`:

```scor
database {
    host = localhost
    port = 5432
}
```

Nodes can nest, so you can build the hierarchy your application expects:

```scor
server {
    tls {
        enabled = true
        certificate = cert.pem
    }
}
```

A node may carry a **header** -- a single value after the name, before the
braces:

```scor
server "primary" {
    port = 8080
}
```

The meaning of a header is up to the embedding application. To Scorium it is
just a string the host receives alongside the node name.

---

## 2. Values

Leaves hold typed values. Scorium understands these built-in types:

| Type | Example | Notes |
| --- | --- | --- |
| Integer | `8080` | |
| Float | `1.5` | Always printed with a decimal point. |
| Boolean | `true`, `false` | |
| Nil | `nil` | The empty value. |
| Bare string | `localhost` | A single unquoted token. |
| Quoted string | `"$HOME/example"` | Literal -- no interpolation. |
| List | `[one, two, "three four"]` | Holds typed values and expressions. |
| Color | `#8EDDFF`, `#101820CC` | RGBA. `#RRGGBB` or `#RRGGBBAA`. |
| Duration | `600ms`, `1.5s`, `2m` | Units are required; never guessed. |

### Bare strings vs. quoted strings

A single unquoted token is normally a string. This is what keeps Scorium
readable -- you rarely need quotes:

```scor
shell = zsh
key = SUPER+Return
action = spawn:terminal
certificate = cert.pem
```

Use a quoted string when the value contains characters that would otherwise
be special (spaces, quotes, or characters that look like operators):

```scor
path = "$HOME/example"
greeting = "hello world"
```

Quoted strings are **literal**: `$name` interpolation does not happen inside
them (see [Variables](#3-variables)).

### Colors and durations are real values

Colors and durations are not strings. They carry real typed data, which is
what makes color methods and duration arithmetic meaningful:

```scor
theme {
    primary = #8EDDFF
    deep = primary.darken(0.25)
}
```

### Lists

Lists hold any combination of typed values:

```scor
servers = [alpha, beta, "gamma delta"]
ports = [8001, 8002, 8003]
```

---

## 3. Variables

Variables remove repetition. Three rules cover almost everything:

| Where | Form | Meaning |
| --- | --- | --- |
| Definition | `@name = value` | Defines a variable. `@` appears **only** here. |
| In a bare string | `$name` | Interpolates the value into the string. |
| In an expression | `name` | References the typed value. |

```scor
@mod = SUPER
@base = 8

# `$mod` interpolates into a bare string:
binding = $mod+Return

# `base` (no marker) is the value in an expression:
gaps = base * 2
```

### Rules

- `@name` **defines** a variable. It only appears on its own line.
- `$name` **interpolates** into a bare string.
- `name` **references** a typed value inside an expression.
- `@name` inside an expression is an error.
- `$name` inside an expression is an error.
- `$name` that is not defined is an error (undefined interpolation).
- Quoted strings stay **literal** -- `$name` inside one is just text.
- Definitions are visible after their declaration.

### Diagnostics teach the form

When you mix the markers up, the diagnostic names the fix:

```
`$base` cannot be used in an expression.
Use `base` for an expression value.
```

```
`mod` is not defined for string interpolation.
Define it first with `@mod = ...`.
```

---

## 4. Expressions

Expressions appear on the right-hand side of `=` (and inside `if` and `while`
conditions, loop ranges, and function calls). They support a safe subset of
Lua.

### Arithmetic

```scor
size = base * 2
total = a + b - c
remainder = n % 10
```

Operators require spaces around them. `base*2` (squeezed against an operand)
is rejected with a diagnostic asking you to write `base * 2`. This avoids the
silent, confusing results that come from a token like `base*2` being read as
a bare string.

### Comparison and boolean logic

```scor
enabled = environment == production
ready = count > 0 and enabled
fallback = primary or secondary
```

| Category | Operators |
| --- | --- |
| Arithmetic | `+  -  *  /  %` |
| Comparison | `==  ~=  <  >  <=  >=` |
| Boolean | `and  or  not` |

`and` and `or` use Lua semantics: they return one of their operands rather
than a boolean.

### Function calls

```scor
size = double(base)
terminal = select(kitty, alacritty, foot)
deep = primary.darken(0.25)
```

A bare word used as a function argument is treated as a string when the
intent is unambiguous -- so `select(kitty, alacritty, foot)` does not force
you to write `select("kitty", "alacritty", "foot")`. Which functions exist is
up to the host application (see [Host APIs](#10-host-apis)).

### Color methods

Colors carry RGBA channels and expose a few methods:

```scor
primary = #8EDDFF
deep    = primary.darken(0.35)   # 0.0..1.0
light   = primary.lighten(0.15)  # 0.0..1.0
faded   = primary.alpha(0.5)     # 0.0..1.0
```

### Sibling access

Inside a node body, an earlier leaf in the **same** block can be referenced
by its key. This is what makes a one-line palette work:

```scor
theme {
    primary = #8EDDFF
    deep = primary.darken(0.35)
}
```

Sibling access is scoped to the innermost node body, not ancestor blocks.

---

## 5. Conditions

`if` / `elseif` / `else` branch on the truthiness of an expression, exactly
as you would expect from Lua:

```scor
if environment == production then
    server {
        workers = 8
    }
elseif environment == staging then
    server {
        workers = 4
    }
else
    server {
        workers = 2
    }
end
```

Every `if` closes with `end`. Lua truthiness applies: everything is truthy
except `nil` and `false`.

---

## 6. Loops

A numeric `for` generates repeated structure. The range is **inclusive**:

```scor
for i = 1, 9 do
    workspace {
        id = workspace-$i
        index = i
    }
end
```

An optional third expression is the step:

```scor
for i = 0, 10, 2 do
    even {
        value = i
    }
end
```

`while` loops while a condition holds:

```scor
local i = 0
while i < 3 do
    item {
        index = i
    }
    i = i + 1
end
```

Loop variables and `local` variables are typed values in expressions, and
interpolate with `$name` in bare strings (`workspace-$i`).

Loops are bounded: the runtime caps the total number of iterations across
one evaluation as a sandbox limit, so a misbehaving config cannot hang
forever.

---

## 7. Functions

A friendly Scorium form lets you define reusable macros without writing raw
Lua:

```scor
fn service(name, port) {
    server {
        id = $name
        port = port
    }
}

service(web, 8080)
service(db, 5432)
```

`$name` interpolates a parameter into a bare string; `port` references the
typed value. The body is ordinary Scorium -- nodes, leaves, even more loops.

`return` exits a function early and can carry a value:

```scor
fn double(x) {
    return x * 2
}
total = double(5)
```

---

## 8. Includes

`include` pulls in another `.scor` file:

```scor
include "theme.scor"
```

Rules:

- Relative paths resolve relative to the **including** file's directory.
- Include cycles are detected and reported with the include chain.
- A host may disable includes, or forbid paths that traverse `..` above the
  including file's directory.
- Included definitions share a documented environment with the includer.
- Include behaviour is deterministic: the same files always produce the same
  result.

---

## 9. Scripts

For raw logic that does not fit a Scorium `fn`, `script { }` runs a selected
subset of Lua against a sandboxed runtime:

```scor
script {
    local n = 0
    for k, v in pairs(widgets) do
        n = n + 1
    end
}
```

`script` bodies are **never reformatted** by `scorium fmt` -- the formatter
does not understand Lua syntax, so it preserves the body byte-for-byte.

### What the sandbox forbids

`script` blocks run with `math`, `string`, and `table` only. There is no
`io`, no `os`, no `package`, no `debug`, no process spawning, no filesystem
access, no networking, no dynamic native-module loading, and no unrestricted
environment-variable access. See [Security limitations](#11-security-limitations).

---

## 10. Host APIs

Most configuration never touches a host API -- nodes and leaves are just data
the host walks afterward. But a host application can extend Scorium by
registering:

- **Host values**, reachable as plain identifiers in expressions (for example
  an `environment` identifier that resolves to a host-supplied string).
- **Host functions**, callable from expressions (`select(kitty, foot)`) and
  from `script { }` blocks alike.

Both go through the **same registry**, so a host never implements an
operation twice -- once for the declarative surface and once for the scripted
one. Which values and functions exist is entirely up to the embedding
application; the standalone `scorium` CLI attaches none. See
[EMBEDDING.md](./EMBEDDING.md) for the Rust API.

---

## 11. Security limitations

Scorium is sandboxed by design:

- `script { }` exposes only `math`, `string`, `table`.
- No file, process, network, or unrestricted environment-variable access.
- Loop iterations and Lua instructions are bounded.
- Includes obey the host's include policy.

Anything beyond that -- reading a file, spawning a process, calling out to
the OS -- is a **capability the host must explicitly grant**. A plain
`.scor` file cannot grant itself anything. See
[SECURITY.md](./SECURITY.md) for the full model.

---

## Comments

```scor
# a comment
-- also a comment
```

Block comments use the Lua form:

```scor
--[[
    multi-line
    comment
]]
```

`#` is also used for color literals (`#8EDDFF`), but only where a value is
expected -- `color = #8EDDFF` is a color, while a `#` at the start of a
token on its own is a comment. Quoted strings protect comment markers.

### Comment preservation by the formatter

The formatter preserves comments at the granularity the parser tracks:
**leading** comments on their own line(s) above an item, and **one
trailing** comment on the same line after an item. A comment written *inside*
an expression, list, or call's parentheses is not tracked by the AST and is
dropped by the formatter. Line comments (`#` and `--`) are normalized to
`#`; block comments (`--[[ ]]`) are kept as written.

This is a known limitation, documented rather than hidden.
