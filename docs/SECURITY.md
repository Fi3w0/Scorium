# Security Model

Scorium is designed to evaluate configuration from sources that are trusted
enough to *configure* an application but not trusted to run arbitrary code.
This document describes the sandbox, what it forbids, and the host's
responsibilities.

---

## Threat model

A `.scor` file author should be able to express configuration, including
loops, conditions, and functions, **without** gaining the ability to:

- read or write files;
- spawn processes or execute commands;
- open network connections;
- load native modules;
- read arbitrary environment variables;
- consume unbounded CPU or memory.

Anything beyond configuration is a **capability the host grants
explicitly**.

---

## The sandbox

### The Lua state

`script { }` blocks run against an `mlua` (Lua 5.4, vendored) state opened
with **only** these standard libraries:

| Exposed | `math`, `string`, `table` |
| --- | --- |
| **Removed entirely** | `io`, `os`, `package`, `debug`, `loadfile`, `dofile`, `require`, `loadlib` |

There is no `os.execute`, no `io.open`, no `package.loadlib`, no
`debug.getregistry`. The globals table is the only one exposed, and it
contains only the safe libraries plus the host-registered values/functions
in scope for that block. State-wide base functions that bypass an isolated
block environment (`collectgarbage`, `getmetatable`, and `load`) are also
removed.

### Resource limits

| Limit | Default | Controlled by |
| --- | --- | --- |
| Total loop iterations per evaluation | 1,000,000 | `RuntimeOptions::max_loop_iterations` |
| Nested Scorium `fn` calls | 256 | `RuntimeOptions::max_function_call_depth` |
| Lua VM instructions per `script { }` block | 50,000,000 | `RuntimeOptions::max_script_instructions` |
| Lua-owned memory per runtime | 64 MiB | `RuntimeOptions::max_lua_memory_bytes` |

The instruction limit is enforced by an instruction-counting hook that fires
on every 1000th VM instruction; exceeding it raises a `script_error`. The
loop limit is checked once per iteration and raises `loop_budget_exceeded`.
The call-depth limit prevents recursive Scorium functions from overflowing
the host's Rust stack.
Lua's allocator enforces the memory limit across strings, tables, and
compiled chunks; allocation failure is reported as a `script_error`.

### Includes

`include "..."` is mediated by the host's `IncludePolicy`:

- `enabled` (default `true`) -- the host can turn includes off entirely.
- `allow_parent_traversal` (default `false`) -- when `false`, absolute paths,
  `..` path components, and symlinks that resolve outside the including
  file's directory are rejected.
- Include **cycles** are detected and reported with the full include chain.
- Relative paths resolve against the including file's directory.

---

## What a `.scor` file cannot do

- **Filesystem.** No `io`, no file reads/writes, no `loadfile`/`dofile`.
- **Processes.** No `os.execute`, no `os.exit` impact on the host, no
  process spawning.
- **Network.** No sockets, no HTTP, no DNS.
- **Native modules.** No `require`, no `package.loadlib`.
- **Environment.** No `os.getenv`.
- **Debug.** No `debug` library -- no registry or metatable tampering.
- **Unbounded loops.** Both `for`/`while` and `script { }` Lua loops are
  bounded.

---

## Host responsibility

The sandbox caps what a `.scor` file can do **on its own**. It does not cap
what a `.scor` file can ask the **host** to do. A host that registers

```rust
runtime.register_function("shell", |args| {
    /* run a command */
});
```

has handed the config file a shell capability. That is the host's choice, and
the host owns the consequences.

Guidance:

- **Register only safe functions.** Treat every registered function as a
  capability you are granting the config file.
- **Prefer pure functions.** Functions that compute a value from their
  arguments are safe by construction.
- **Tighten include policy for untrusted input.** Start from
  `IncludePolicy::default()` or disable includes.
- **Evaluate in a disposable context** if you accept truly untrusted
  configuration, in case of a panic from a parser bug.

---

## Reporting vulnerabilities

See the root [`SECURITY.md`](../SECURITY.md) for how to report a
vulnerability privately. In short: do **not** open a public issue for a
security problem; use the project's private security contact.

In scope for the security policy:

- Any way a `.scor` file escapes the sandbox (files, processes, network,
  native modules, secret env vars, unbounded resources).
- Crashes or panics reachable from parsing or evaluating untrusted input.
- Bypass of the loop or instruction budgets.
- Include path-traversal or cycle-detection bypass.

Out of scope:

- Issues that require the host to have already registered a dangerous
  function. That is the host's security decision.
- DoS that requires the operator to already trust the file.

---

## No `unsafe`

The library crates contain no `unsafe` code. (The `mlua` FFI into Lua uses
`unsafe` internally, as any FFI must; that is `mlua`'s responsibility, not
Scorium's.)
